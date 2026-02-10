#!/usr/bin/env python3
"""2-player LOS/raycast debug bot for games/raycast-debug."""

from __future__ import annotations

import argparse
import math
import pathlib
import time
from dataclasses import dataclass
from typing import Any

import requests

DEFAULT_API_BASE = "http://localhost:8080/api/v1"
DEFAULT_BOT_PREFIX = "raycast-debug-bot"
REQUEST_TIMEOUT = 10


@dataclass
class Session:
    name: str
    game_id: str
    api_base: str
    headers: dict[str, str]


ARENA_MIN = -36.0
ARENA_MAX = 36.0


def clamp_target(pos: list[float], x: float, z: float) -> list[float]:
    return [
        max(ARENA_MIN, min(ARENA_MAX, x)),
        float(pos[1] if len(pos) > 1 else 2.0),
        max(ARENA_MIN, min(ARENA_MAX, z)),
    ]


def near_boundary(pos: list[float], margin: float = 3.0) -> bool:
    x = float(pos[0])
    z = float(pos[2])
    return (
        x <= ARENA_MIN + margin
        or x >= ARENA_MAX - margin
        or z <= ARENA_MIN + margin
        or z >= ARENA_MAX - margin
    )


def read_local_game_id() -> str | None:
    game_id_file = pathlib.Path(__file__).resolve().parent / ".clawblox" / "game_id"
    if not game_id_file.exists():
        return None
    value = game_id_file.read_text(encoding="utf-8").strip()
    return value or None


def register_agent(api_base: str, name: str) -> str:
    resp = requests.post(
        f"{api_base}/agents/register",
        json={"name": name, "description": "raycast debug test bot"},
        timeout=REQUEST_TIMEOUT,
    )
    if resp.status_code not in (200, 201):
        raise RuntimeError(f"register failed name={name}: {resp.status_code} {resp.text}")
    body = resp.json()
    api_key = body.get("api_key")
    if (not isinstance(api_key, str) or not api_key) and isinstance(body.get("agent"), dict):
        api_key = body["agent"].get("api_key")
    if not isinstance(api_key, str) or not api_key:
        raise RuntimeError(f"missing api_key for {name}: {body}")
    return api_key


def join_game(api_base: str, game_id: str, api_key: str) -> dict[str, str]:
    headers = {"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"}
    resp = requests.post(f"{api_base}/games/{game_id}/join", headers=headers, timeout=REQUEST_TIMEOUT)
    if resp.status_code != 200:
        raise RuntimeError(f"join failed: {resp.status_code} {resp.text}")
    return headers


def observe(sess: Session) -> dict[str, Any] | None:
    try:
        resp = requests.get(
            f"{sess.api_base}/games/{sess.game_id}/observe",
            headers=sess.headers,
            timeout=REQUEST_TIMEOUT,
        )
        if resp.status_code != 200:
            print(f"[net][{sess.name}] observe status={resp.status_code}")
            return None
        return resp.json()
    except requests.RequestException as exc:
        print(f"[net][{sess.name}] observe error: {exc}")
        return None


def send_input(sess: Session, input_type: str, data: dict[str, Any]) -> bool:
    try:
        payload = {"type": input_type, "data": data}
        resp = requests.post(
            f"{sess.api_base}/games/{sess.game_id}/input",
            headers=sess.headers,
            json=payload,
            timeout=REQUEST_TIMEOUT,
        )
        if resp.status_code != 200:
            print(f"[net][{sess.name}] input status={resp.status_code} type={input_type}")
            return False
        return True
    except requests.RequestException as exc:
        print(f"[net][{sess.name}] input error type={input_type}: {exc}")
        return False


def leave(sess: Session) -> None:
    try:
        requests.post(
            f"{sess.api_base}/games/{sess.game_id}/leave",
            headers=sess.headers,
            timeout=5,
        )
        print(f"[leave] {sess.name}")
    except requests.RequestException as exc:
        print(f"[leave] warning {sess.name}: {exc}")


def main() -> int:
    parser = argparse.ArgumentParser(description="2-player raycast debug bot")
    parser.add_argument("--api-base", default=DEFAULT_API_BASE)
    parser.add_argument("--game-id", default=None)
    parser.add_argument("--duration", type=float, default=40.0)
    parser.add_argument("--tick", type=float, default=0.35)
    parser.add_argument("--num-players", type=int, default=2)
    args = parser.parse_args()

    api_base = args.api_base.rstrip("/")
    game_id = args.game_id or read_local_game_id()
    if not game_id:
        raise RuntimeError("missing game_id; pass --game-id or deploy once to write .clawblox/game_id")

    num_players = max(2, int(args.num_players))
    sessions: list[Session] = []

    try:
        run_id = int(time.time())
        for i in range(num_players):
            name = f"{DEFAULT_BOT_PREFIX}-{run_id}-{i + 1}"
            api_key = register_agent(api_base, name)
            headers = join_game(api_base, game_id, api_key)
            sessions.append(Session(name=name, game_id=game_id, api_base=api_base, headers=headers))
            print(f"[join] {name}")

        waypoints = [
            [28.0, 2.0, 0.0],
            [-28.0, 2.0, 0.0],
            [0.0, 2.0, 28.0],
            [0.0, 2.0, -28.0],
        ]
        wp_idx = [i % len(waypoints) for i in range(num_players)]
        last_move = [0.0 for _ in range(num_players)]
        last_fire = [0.0 for _ in range(num_players)]
        last_pos_by_idx: list[list[float] | None] = [None for _ in range(num_players)]
        escape_side_by_idx = [1.0 for _ in range(num_players)]

        start = time.monotonic()
        while time.monotonic() - start < float(args.duration):
            for i, sess in enumerate(sessions):
                obs = observe(sess)
                if obs is None:
                    continue

                player = obs.get("player") or {}
                pos = player.get("position") or [0, 0, 0]
                if isinstance(pos, list) and len(pos) >= 3:
                    last_pos_by_idx[i] = [float(pos[0]), float(pos[1]), float(pos[2])]
                tick = obs.get("tick")
                other_players = obs.get("other_players")
                visible = other_players if isinstance(other_players, list) else []
                visible_count = len(visible)
                first_dist = None
                if visible_count > 0 and isinstance(visible[0], dict):
                    first_dist = visible[0].get("distance")

                if first_dist is None:
                    print(f"[obs][{sess.name}] tick={tick} pos=({pos[0]:.1f},{pos[1]:.1f},{pos[2]:.1f}) visible={visible_count}")
                else:
                    print(
                        f"[obs][{sess.name}] tick={tick} pos=({pos[0]:.1f},{pos[1]:.1f},{pos[2]:.1f}) "
                        f"visible={visible_count} d0={float(first_dist):.1f}"
                    )

                now = time.monotonic()
                if now - last_move[i] >= 0.55:
                    target = waypoints[wp_idx[i]]
                    # Bot 0 = hunter: chase nearest visible enemy and fire.
                    if i == 0 and visible_count > 0:
                        enemy = visible[0]
                        enemy_pos = enemy.get("position")
                        if isinstance(enemy_pos, list) and len(enemy_pos) >= 3:
                            target = [float(enemy_pos[0]), 2.0, float(enemy_pos[2])]
                            if now - last_fire[i] >= 0.28 and send_input(sess, "Fire", {}):
                                last_fire[i] = now
                                print(f"[fire][{sess.name}] target={enemy.get('name') or enemy.get('id')}")
                    # Bot 1 = runner: move away from nearest visible enemy.
                    elif i == 1 and visible_count > 0:
                        enemy = visible[0]
                        enemy_pos = enemy.get("position")
                        if isinstance(enemy_pos, list) and len(enemy_pos) >= 3:
                            dx = float(pos[0]) - float(enemy_pos[0])
                            dz = float(pos[2]) - float(enemy_pos[2])
                            mag = math.hypot(dx, dz)
                            if mag < 1.0e-4:
                                dx, dz, mag = 1.0, 0.0, 1.0
                            nx, nz = dx / mag, dz / mag
                            flee_dist = 24.0 if mag < 16.0 else 18.0
                            direct = clamp_target(pos, float(pos[0]) + nx * flee_dist, float(pos[2]) + nz * flee_dist)
                            # If we're near arena boundary or direct flee gets clamped too much,
                            # sidestep tangentially to avoid getting pinned.
                            direct_step = math.hypot(direct[0] - float(pos[0]), direct[2] - float(pos[2]))
                            if near_boundary(pos) or direct_step < flee_dist * 0.7:
                                side = escape_side_by_idx[i]
                                tx = -nz * side
                                tz = nx * side
                                tangent = clamp_target(
                                    pos,
                                    float(pos[0]) + nx * 8.0 + tx * 20.0,
                                    float(pos[2]) + nz * 8.0 + tz * 20.0,
                                )
                                target = tangent
                                escape_side_by_idx[i] *= -1.0
                            else:
                                target = direct
                    # Runner fallback: flee hunter from shared observation cache even if not visible.
                    elif i == 1 and len(last_pos_by_idx) > 0 and last_pos_by_idx[0] is not None:
                        hunter_pos = last_pos_by_idx[0]
                        dx = float(pos[0]) - float(hunter_pos[0])
                        dz = float(pos[2]) - float(hunter_pos[2])
                        mag = math.hypot(dx, dz)
                        if mag < 1.0e-4:
                            dx, dz, mag = 1.0, 0.0, 1.0
                        nx, nz = dx / mag, dz / mag
                        target = clamp_target(pos, float(pos[0]) + nx * 16.0, float(pos[2]) + nz * 16.0)
                    if send_input(sess, "MoveTo", {"position": target}):
                        if (i == 0 and visible_count == 0) or (i == 1 and visible_count == 0) or i >= 2:
                            wp_idx[i] = (wp_idx[i] + 1) % len(waypoints)
                        last_move[i] = now
                        print(
                            f"[move][{sess.name}] target=({target[0]:.1f},{target[1]:.1f},{target[2]:.1f})"
                        )

            time.sleep(float(args.tick))

        print("[done] duration reached")
        return 0
    finally:
        for sess in sessions:
            leave(sess)


if __name__ == "__main__":
    raise SystemExit(main())
