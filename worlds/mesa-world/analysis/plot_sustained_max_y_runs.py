#!/usr/bin/env python3
"""Plot sustained max agent Y over time for multiple runs."""

from __future__ import annotations

import argparse
from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare sustained max agent Y over time across multiple runs."
    )
    parser.add_argument(
        "run_dirs",
        nargs="*",
        type=Path,
        help=(
            "Run directories (e.g., results/20260311T032536Z_run1). "
            "If omitted, auto-discovers *_run* under --results-dir."
        ),
    )
    parser.add_argument(
        "--results-dir",
        type=Path,
        default=Path("results"),
        help="Base directory used for auto-discovery (default: results).",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output PNG path.",
    )
    parser.add_argument(
        "--figsize",
        nargs=2,
        type=float,
        default=(5.2, 3.2),
        metavar=("W", "H"),
        help="Figure size in inches (default: 5.2 3.2).",
    )
    parser.add_argument(
        "--max-time-min",
        type=float,
        default=60.0,
        help="Truncate plot at this time in minutes (default: 60).",
    )
    return parser.parse_args()


def discover_run_dirs(results_dir: Path) -> list[Path]:
    return sorted(p for p in results_dir.glob("*_run*") if p.is_dir())


def sustained_csv_for_run(run_dir: Path) -> Path:
    matches = sorted(
        run_dir.glob("worlds/world-0/recordings/*_analysis/agent_max_y_sustained.csv")
    )
    if not matches:
        raise FileNotFoundError(
            f"No agent_max_y_sustained.csv found under run dir: {run_dir}"
        )
    return matches[-1]


def agent_csv_for_run(run_dir: Path) -> Path:
    matches = sorted(run_dir.glob("worlds/world-0/recordings/*_analysis/agent_vertical.csv"))
    if not matches:
        raise FileNotFoundError(f"No agent_vertical.csv found under run dir: {run_dir}")
    return matches[-1]


def agent_start_time_s(agent_csv: Path) -> float:
    df = pd.read_csv(agent_csv, usecols=["time_s", "agent_y"])
    time_s = pd.to_numeric(df["time_s"], errors="coerce")
    agent_y = pd.to_numeric(df["agent_y"], errors="coerce")
    mask = time_s.notna() & agent_y.notna()
    if not mask.any():
        print(f"[warn] No valid agent_y rows in {agent_csv}; using t0=0.")
        return 0.0
    return float(time_s[mask].iloc[0])


def load_series(csv_path: Path, t0_s: float) -> tuple[pd.Series, pd.Series]:
    df = pd.read_csv(csv_path, usecols=["time_s", "max_agent_y_sustained_2s"])

    sustained_col = next(
        (c for c in df.columns if c.startswith("max_agent_y_sustained")),
        None,
    )
    if sustained_col is None:
        raise ValueError(f"No sustained max column found in: {csv_path}")

    x = (pd.to_numeric(df["time_s"], errors="coerce") - t0_s) / 60.0
    y = pd.to_numeric(df[sustained_col], errors="coerce")
    mask = x.notna() & (x >= 0.0)
    return x[mask].reset_index(drop=True), y[mask].reset_index(drop=True)


def main() -> None:
    args = parse_args()
    script_dir = Path(__file__).resolve().parent
    output_path = (
        args.output.resolve()
        if args.output is not None
        else script_dir / "results" / "max_y_sustained_compare.png"
    )

    run_dirs = [p.resolve() for p in args.run_dirs] if args.run_dirs else []
    if not run_dirs:
        run_dirs = [p.resolve() for p in discover_run_dirs(args.results_dir.resolve())]

    if not run_dirs:
        raise RuntimeError("No run directories found. Provide them explicitly or check --results-dir.")

    plt.figure(figsize=tuple(args.figsize))
    run_series = []

    for idx, run_dir in enumerate(run_dirs, start=1):
        sustained_csv = sustained_csv_for_run(run_dir)
        agent_csv = agent_csv_for_run(run_dir)
        t0_s = agent_start_time_s(agent_csv)

        x, y = load_series(sustained_csv, t0_s=t0_s)
        if x.empty:
            print(f"[warn] No time samples in {sustained_csv}; skipping.")
            continue

        finite_mask = y.notna()
        if not finite_mask.any():
            print(f"[warn] No sustained-max values in {sustained_csv}; skipping.")
            continue

        run_series.append(
            {
                "label": f"generation {idx}",
                "run_name": run_dir.name,
                "x": x,
                "y": y,
                "end_t": float(x.iloc[-1]),
                "final_sustained": float(y[finite_mask].iloc[-1]),
                "peak_sustained": float(y[finite_mask].max()),
                "t0_s": t0_s,
            }
        )

    if not run_series:
        raise RuntimeError("No runs had plottable sustained-max samples.")

    global_end_t = min(max(r["end_t"] for r in run_series), args.max_time_min)
    # Muted, high-contrast palette (avoids bright default-like tones).
    palette = ["#264653", "#2A9D8F", "#E76F51", "#6D597A", "#8AB17D", "#457B9D"]
    plotted = 0
    for i, series in enumerate(run_series):
        x_finite = series["x"][series["y"].notna()].reset_index(drop=True)
        y_finite = series["y"][series["y"].notna()].reset_index(drop=True)
        if x_finite.empty:
            continue

        # Drop samples beyond the requested time window.
        within_window = x_finite <= global_end_t
        x_finite = x_finite[within_window].reset_index(drop=True)
        y_finite = y_finite[within_window].reset_index(drop=True)
        if x_finite.empty:
            continue

        if float(x_finite.iloc[-1]) < global_end_t:
            x_finite = pd.concat([x_finite, pd.Series([global_end_t])], ignore_index=True)
            y_finite = pd.concat([y_finite, pd.Series([float(y_finite.iloc[-1])])], ignore_index=True)

        color = palette[i % len(palette)]
        plt.step(
            x_finite,
            y_finite,
            where="post",
            linewidth=1.8,
            color=color,
            label=series["label"],
        )
        print(
            f"[ok] {series['run_name']} ({series['label']}): t0={series['t0_s']:.3f}s, "
            f"final/peak sustained y={series['final_sustained']:.3f}/{series['peak_sustained']:.3f}, "
            f"extended_to={global_end_t:.3f}min"
        )
        plotted += 1

    if plotted == 0:
        raise RuntimeError("No runs had plottable sustained-max samples.")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    plt.xlabel("Time (min)")
    plt.ylabel("Max height reached")
    ax = plt.gca()
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.set_xlim(0, args.max_time_min)
    plt.legend()
    plt.tight_layout()
    plt.savefig(output_path, dpi=160)
    plt.close()

    print(f"[done] Wrote figure: {output_path}")


if __name__ == "__main__":
    main()
