#!/usr/bin/env python3
import argparse
import csv
import io
import json
import math
import sqlite3
from collections import defaultdict
from pathlib import Path
from statistics import median

import matplotlib.pyplot as plt
import zstandard as zstd


def parse_args():
    parser = argparse.ArgumentParser(
        description="Analyze a .clawrec recording and plot agent vertical position + block stack height over time."
    )
    parser.add_argument("recording", type=Path, help="Path to .clawrec file")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Output directory for CSV/PNG files (default: <recording_stem>_analysis)",
    )
    parser.add_argument(
        "--agent-name",
        type=str,
        default=None,
        help="Track this exact agent/player name (default: first seen player)",
    )
    parser.add_argument(
        "--agent-id",
        type=str,
        default=None,
        help="Track this exact agent UUID (default: first seen player)",
    )
    parser.add_argument(
        "--tick-step",
        type=int,
        default=1,
        help="Sample every N ticks in output (default: 1)",
    )
    parser.add_argument(
        "--max-tick",
        type=int,
        default=None,
        help="Stop analysis at this tick",
    )
    parser.add_argument(
        "--stack-bin-size",
        type=float,
        default=None,
        help="XZ bin size for stack grouping (default: auto from block size)",
    )
    return parser.parse_args()


def decode_blob(blob: bytes):
    # Some frames omit uncompressed size in the header; stream_reader handles both cases.
    dctx = zstd.ZstdDecompressor()
    with dctx.stream_reader(io.BytesIO(blob)) as reader:
        decompressed = reader.read()
    return json.loads(decompressed)


def is_block_entity(entity: dict) -> bool:
    attrs = entity.get("attributes") or {}
    if attrs.get("IsPickupBlock") is True:
        return True
    render = entity.get("render") or {}
    if render.get("role") == "Block":
        return True
    return False


def compute_max_stack(observation: dict, explicit_bin_size: float | None) -> tuple[int, int]:
    blocks = []
    for ent in observation.get("entities", []):
        if not is_block_entity(ent):
            continue
        pos = ent.get("position")
        size = ent.get("size")
        if not pos or not size:
            continue
        try:
            x, y, z = float(pos[0]), float(pos[1]), float(pos[2])
            sy = float(size[1]) if len(size) > 1 else 0.75
        except Exception:
            continue
        blocks.append((x, y, z, max(sy, 0.01)))

    if not blocks:
        return 0, 0

    heights = sorted(b[3] for b in blocks)
    nominal_h = heights[len(heights) // 2] if heights else 0.75
    if nominal_h <= 0:
        nominal_h = 0.75

    bin_size = explicit_bin_size if explicit_bin_size and explicit_bin_size > 0 else nominal_h * 0.65
    min_gap = nominal_h * 0.45
    max_gap = nominal_h * 1.75

    columns = defaultdict(list)
    for x, y, z, _ in blocks:
        key = (int(round(x / bin_size)), int(round(z / bin_size)))
        columns[key].append(y)

    best = 1
    for ys in columns.values():
        ys.sort()
        cur = 1
        col_best = 1
        for i in range(1, len(ys)):
            dy = ys[i] - ys[i - 1]
            if min_gap <= dy <= max_gap:
                cur += 1
            else:
                cur = 1
            if cur > col_best:
                col_best = cur
        if col_best > best:
            best = col_best

    return best, len(blocks)


def apply_delta(observation: dict, delta: dict):
    entities = observation.get("entities", [])
    by_entity_id = {int(e["id"]): e for e in entities if "id" in e}

    for entry in delta.get("entities", []):
        if "add" in entry:
            add_ent = entry["add"]
            by_entity_id[int(add_ent["id"])] = add_ent
        elif "rm" in entry and entry.get("rm"):
            by_entity_id.pop(int(entry["id"]), None)
        else:
            ent = by_entity_id.get(int(entry["id"]))
            if ent is None:
                continue
            if "p" in entry and entry["p"] is not None:
                ent["position"] = entry["p"]
            if "r" in entry and entry["r"] is not None:
                ent["rotation"] = entry["r"]

    observation["entities"] = list(by_entity_id.values())

    players = observation.get("players", [])
    by_player_id = {p["id"]: p for p in players if "id" in p}
    for pd in delta.get("players", []):
        player = by_player_id.get(pd.get("id"))
        if player is None:
            continue
        if pd.get("p") is not None:
            player["position"] = pd["p"]
        if pd.get("health") is not None:
            player["health"] = pd["health"]

    observation["players"] = list(by_player_id.values())
    observation["sounds"] = delta.get("sounds", [])
    observation["recent_events"] = delta.get("recent_events", [])


def choose_agent(players: list[dict], explicit_name: str | None, explicit_id: str | None):
    if explicit_id:
        for p in players:
            if p.get("id") == explicit_id:
                return explicit_id, p.get("name", explicit_id)
        return None, None
    if explicit_name:
        for p in players:
            if p.get("name") == explicit_name:
                return p.get("id"), explicit_name
        return None, None
    if not players:
        return None, None
    players_sorted = sorted(players, key=lambda p: p.get("name", ""))
    chosen = players_sorted[0]
    return chosen.get("id"), chosen.get("name", chosen.get("id"))


def plot_series(
    x_values: list[float],
    y_values: list[float],
    ylabel: str,
    title: str,
    output_png: Path,
):
    plt.figure(figsize=(12, 4))
    plt.plot(x_values, y_values, linewidth=1.2)
    plt.xlabel("Time (s)")
    plt.ylabel(ylabel)
    plt.title(title)
    plt.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.savefig(output_png, dpi=150)
    plt.close()


def main():
    args = parse_args()
    recording = args.recording.resolve()
    if not recording.exists():
        raise FileNotFoundError(f"Recording not found: {recording}")

    out_dir = args.output_dir or recording.with_name(f"{recording.stem}_analysis")
    out_dir.mkdir(parents=True, exist_ok=True)

    conn = sqlite3.connect(str(recording))
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    meta = {
        row["key"]: row["value"]
        for row in cur.execute("SELECT key, value FROM meta").fetchall()
    }
    tick_rate = int(meta.get("tick_rate", "60"))

    cp_ticks = [int(r["tick"]) for r in cur.execute("SELECT tick FROM checkpoints ORDER BY tick").fetchall()]
    if not cp_ticks:
        raise RuntimeError("No checkpoints in recording.")
    first_tick = cp_ticks[0]

    db_max_tick_row = cur.execute(
        """
        SELECT COALESCE(MAX(t), 0) AS max_tick
        FROM (
          SELECT MAX(tick) AS t FROM checkpoints
          UNION ALL
          SELECT MAX(tick) AS t FROM deltas
        )
        """
    ).fetchone()
    db_max_tick = int(db_max_tick_row["max_tick"]) if db_max_tick_row else first_tick
    meta_total_ticks = int(meta.get("total_ticks", "0") or "0")
    end_tick = max(db_max_tick, meta_total_ticks)
    if args.max_tick is not None:
        end_tick = min(end_tick, args.max_tick)

    agent_csv = out_dir / "agent_vertical.csv"
    stack_csv = out_dir / "max_stack.csv"
    y_plot_png = out_dir / "agent_vertical.png"
    stack_plot_png = out_dir / "max_stack.png"
    summary_json = out_dir / "summary.json"

    y_times = []
    y_vals = []
    stack_times = []
    stack_vals = []

    selected_agent_id = args.agent_id
    selected_agent_name = args.agent_name
    auto_select_announced = False

    max_stack_seen = 0
    max_stack_tick = None
    max_y_seen = None
    max_y_tick = None

    with agent_csv.open("w", newline="") as f_agent, stack_csv.open("w", newline="") as f_stack:
        agent_writer = csv.writer(f_agent)
        stack_writer = csv.writer(f_stack)
        agent_writer.writerow(["tick", "time_s", "agent_id", "agent_name", "agent_y"])
        stack_writer.writerow(["tick", "time_s", "max_stacked_blocks", "num_blocks"])

        for idx, cp_tick in enumerate(cp_ticks):
            if cp_tick > end_tick:
                break
            next_cp = cp_ticks[idx + 1] if idx + 1 < len(cp_ticks) else end_tick + 1
            segment_end = min(end_tick, next_cp - 1)
            if segment_end < cp_tick:
                continue

            cp_row = cur.execute(
                "SELECT tick, time_ms, data FROM checkpoints WHERE tick = ?",
                (cp_tick,),
            ).fetchone()
            if cp_row is None:
                continue

            observation = decode_blob(cp_row["data"])
            time_ms = int(cp_row["time_ms"])

            def sample_now(tick: int, ms: int, obs: dict):
                nonlocal selected_agent_id, selected_agent_name
                nonlocal auto_select_announced, max_stack_seen, max_stack_tick, max_y_seen, max_y_tick

                if tick < first_tick:
                    return
                if tick != end_tick and ((tick - first_tick) % args.tick_step != 0):
                    return

                players = obs.get("players", [])
                if not selected_agent_id:
                    aid, aname = choose_agent(players, args.agent_name, args.agent_id)
                    if aid:
                        selected_agent_id = aid
                        selected_agent_name = aname
                        if args.agent_name is None and args.agent_id is None and not auto_select_announced:
                            print(f"[info] Auto-selected agent: {selected_agent_name} ({selected_agent_id})")
                            auto_select_announced = True

                agent_y = None
                if selected_agent_id:
                    for p in players:
                        if p.get("id") == selected_agent_id:
                            pos = p.get("position") or []
                            if len(pos) >= 2:
                                agent_y = float(pos[1])
                            break

                max_stack, num_blocks = compute_max_stack(obs, args.stack_bin_size)
                t_s = ms / 1000.0

                agent_writer.writerow(
                    [
                        tick,
                        f"{t_s:.6f}",
                        selected_agent_id or "",
                        selected_agent_name or "",
                        "" if agent_y is None else f"{agent_y:.6f}",
                    ]
                )
                stack_writer.writerow([tick, f"{t_s:.6f}", max_stack, num_blocks])

                if agent_y is not None:
                    y_times.append(t_s)
                    y_vals.append(agent_y)
                    if max_y_seen is None or agent_y > max_y_seen:
                        max_y_seen = agent_y
                        max_y_tick = tick

                stack_times.append(t_s)
                stack_vals.append(float(max_stack))
                if max_stack > max_stack_seen:
                    max_stack_seen = max_stack
                    max_stack_tick = tick

            sample_now(int(cp_row["tick"]), time_ms, observation)

            delta_rows = cur.execute(
                """
                SELECT tick, time_ms, data
                FROM deltas
                WHERE tick > ? AND tick <= ?
                ORDER BY tick
                """,
                (cp_tick, segment_end),
            ).fetchall()

            for row in delta_rows:
                tick = int(row["tick"])
                time_ms = int(row["time_ms"])
                delta = decode_blob(row["data"])
                apply_delta(observation, delta)
                observation["tick"] = tick
                observation["server_time_ms"] = time_ms
                sample_now(tick, time_ms, observation)

    if y_times and y_vals:
        plot_series(
            y_times,
            y_vals,
            ylabel="Agent Y Position",
            title=f"Agent Vertical Position Over Time ({selected_agent_name or 'agent'})",
            output_png=y_plot_png,
        )
    else:
        print("[warn] No agent Y samples found; agent_vertical.png not generated.")

    if stack_times and stack_vals:
        plot_series(
            stack_times,
            stack_vals,
            ylabel="Max Stacked Blocks",
            title="Max Number of Blocks Stacked Over Time",
            output_png=stack_plot_png,
        )
    else:
        print("[warn] No stack samples found; max_stack.png not generated.")

    summary = {
        "recording": str(recording),
        "tick_rate": tick_rate,
        "first_tick": first_tick,
        "end_tick": end_tick,
        "agent_id": selected_agent_id,
        "agent_name": selected_agent_name,
        "max_agent_y": max_y_seen,
        "max_agent_y_tick": max_y_tick,
        "max_stack_seen": max_stack_seen,
        "max_stack_tick": max_stack_tick,
        "outputs": {
            "agent_csv": str(agent_csv),
            "stack_csv": str(stack_csv),
            "agent_plot_png": str(y_plot_png) if y_times else None,
            "stack_plot_png": str(stack_plot_png) if stack_times else None,
        },
    }
    summary_json.write_text(json.dumps(summary, indent=2))

    print(f"[done] Wrote analysis to: {out_dir}")
    print(f"[done] Agent CSV: {agent_csv}")
    print(f"[done] Stack CSV: {stack_csv}")
    if y_times:
        print(f"[done] Agent plot: {y_plot_png}")
    if stack_times:
        print(f"[done] Stack plot: {stack_plot_png}")
    print(f"[done] Summary: {summary_json}")


if __name__ == "__main__":
    main()
