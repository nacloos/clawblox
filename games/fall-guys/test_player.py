#!/usr/bin/env python3
"""
Test player for Fall Guys - navigates the obstacle course automatically.

Usage:
  # Local clawblox run server
  python test_player.py --api-base http://localhost:8080

  # Hosted server
  python test_player.py --api-base http://localhost:8080/api/v1 --game-id <uuid>
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import requests

AGENT_NAME = "fallguys-test-player"

# Course layout from SKILL.md (scaled 4x)
S = 4
DISC_CENTERS = [
    (0 * S, 26 * S), (3 * S, 38 * S), (-2 * S, 50 * S),
    (1 * S, 62 * S), (-1 * S, 74 * S), (2 * S, 84 * S),
]
FINISH_Z = 250 * S


@dataclass
class Session:
    mode: str  # "local" | "platform"
    api_base: str
    game_id: str | None
    headers: dict[str, str]


def key_cache_path() -> Path:
    return Path.home() / ".clawblox" / "fallguys_test_keys.json"


def load_cached_api_key(api_base: str) -> str | None:
    try:
        data = json.loads(key_cache_path().read_text())
        return data.get(api_base)
    except Exception:
        return None


def save_cached_api_key(api_base: str, api_key: str) -> None:
    path = key_cache_path()
    data: dict[str, str] = {}
    try:
        data = json.loads(path.read_text())
    except Exception:
        pass
    data[api_base] = api_key
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2))


def register_agent(api_base: str) -> str:
    r = requests.post(
        f"{api_base}/agents/register",
        json={"name": AGENT_NAME, "description": "Fall Guys test player"},
        timeout=10,
    )
    r.raise_for_status()
    body = r.json()
    key = body.get("api_key")
    if not key:
        agent = body.get("agent", {})
        key = agent.get("api_key")
    if not key:
        raise RuntimeError("register response missing api_key")
    return key


def detect_mode(api_base: str) -> str:
    return "platform" if api_base.endswith("/api/v1") else "local"


def create_session(args: argparse.Namespace) -> Session:
    api_base = args.api_base.rstrip("/")
    mode = detect_mode(api_base)

    if mode == "platform":
        if not args.game_id:
            raise RuntimeError("--game-id required for /api/v1 mode")
        api_key = args.api_key or load_cached_api_key(api_base)
        if not api_key:
            api_key = register_agent(api_base)
            save_cached_api_key(api_base, api_key)
            print(f"[auth] registered and cached key")
        headers = {"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"}
        r = requests.post(f"{api_base}/games/{args.game_id}/join", headers=headers, timeout=10)
        r.raise_for_status()
        print(f"[join] joined game (platform mode)")
        return Session(mode="platform", api_base=api_base, game_id=args.game_id, headers=headers)
    else:
        # Local mode - wait for server
        deadline = time.time() + 12.0
        while time.time() < deadline:
            try:
                ping = requests.get(f"{api_base}/skill.md", timeout=2)
                if ping.status_code in (200, 404):
                    break
            except Exception:
                pass
            time.sleep(0.25)

        r = requests.post(f"{api_base}/join", params={"name": AGENT_NAME}, timeout=10)
        r.raise_for_status()
        token = r.json().get("session", "")
        headers = {"X-Session": token, "Content-Type": "application/json"}
        print(f"[join] joined local game session={token[:8]}...")
        return Session(mode="local", api_base=api_base, game_id=args.game_id, headers=headers)


def observe(sess: Session) -> dict | None:
    try:
        if sess.mode == "platform":
            r = requests.get(f"{sess.api_base}/games/{sess.game_id}/observe", headers=sess.headers, timeout=10)
        else:
            r = requests.get(f"{sess.api_base}/observe", headers=sess.headers, timeout=10)
        r.raise_for_status()
        return r.json()
    except Exception as e:
        print(f"[observe] error: {e}")
        return None


def send_input(sess: Session, input_type: str, data: dict | None = None) -> bool:
    if data is None:
        data = {}
    payload = {"type": input_type, "data": data}
    try:
        if sess.mode == "platform":
            r = requests.post(f"{sess.api_base}/games/{sess.game_id}/input", headers=sess.headers, json=payload, timeout=10)
        else:
            r = requests.post(f"{sess.api_base}/input", headers=sess.headers, json=payload, timeout=10)
        r.raise_for_status()
        return True
    except Exception as e:
        print(f"[input] error: {e}")
        return False


def leave(sess: Session) -> None:
    try:
        if sess.mode == "platform":
            requests.post(f"{sess.api_base}/games/{sess.game_id}/leave", headers=sess.headers, timeout=5)
        print("[leave] left game")
    except Exception:
        pass


def get_section(z: float) -> int:
    if z < 22 * S:
        return 0
    elif z < 92 * S:
        return 1
    elif z < 182 * S:
        return 2
    return 3


def next_disc_target(z: float) -> tuple[float, float] | None:
    """Find the next disc center ahead of current Z."""
    for cx, cz in DISC_CENTERS:
        if cz > z - 2 * S:
            return (cx, cz)
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Fall Guys test player")
    parser.add_argument("--api-base", default="http://localhost:8080", help="Server URL")
    parser.add_argument("--game-id", default=None, help="Game UUID (required for /api/v1)")
    parser.add_argument("--api-key", default=None, help="Existing API key")
    parser.add_argument("--duration", type=float, default=120.0, help="Run duration in seconds")
    parser.add_argument("--tick", type=float, default=0.25, help="Loop interval")
    args = parser.parse_args()

    print("=" * 50)
    print("Fall Guys Test Player")
    print("=" * 50)

    sess = create_session(args)
    time.sleep(1.0)

    start = time.time()
    last_jump = 0.0
    last_section = -1
    finished = False

    try:
        while time.time() - start < args.duration and not finished:
            now = time.time()
            obs = observe(sess)
            if obs is None:
                time.sleep(0.5)
                continue

            player = obs.get("player", {})
            pos = player.get("position", [0, 0, 0])
            attrs = player.get("attributes", {})
            px, py, pz = pos[0], pos[1], pos[2]

            section = get_section(pz)
            game_state = attrs.get("GameState", "unknown")
            place = attrs.get("Place", "?")
            progress = attrs.get("Progress", 0)
            timer = attrs.get("Timer", "")

            if section != last_section:
                section_names = ["Start", "Spinning Discs", "Pendulum Bridge", "Hex-a-Gone"]
                name = section_names[section] if section < len(section_names) else "?"
                print(f"\n>>> Entering Section {section}: {name}")
                last_section = section

            print(
                f"\r[{timer}] pos=({px:6.1f}, {py:5.1f}, {pz:6.1f}) "
                f"section={section} place={place} progress={progress}% state={game_state}  ",
                end="", flush=True,
            )

            if game_state == "finished":
                finish_time = attrs.get("FinishTime", timer)
                print(f"\n\nFINISHED! Place: {place}, Time: {finish_time}")
                finished = True
                break

            if game_state != "playing":
                time.sleep(0.5)
                continue

            # Navigation strategy per section
            target_x, target_z = 0.0, pz + 15.0 * S

            if section == 0:
                # Run straight to first disc
                target_x = 0
                target_z = 26 * S

            elif section == 1:
                # Spinning discs: target next disc center
                disc = next_disc_target(pz)
                if disc:
                    target_x, target_z = disc
                    # Jump when between discs (gap)
                    dist_to_disc = math.sqrt((px - target_x) ** 2 + (pz - target_z) ** 2)
                    if dist_to_disc > 3.0 * S and now - last_jump > 0.8:
                        send_input(sess, "Jump")
                        last_jump = now
                else:
                    target_x = 0
                    target_z = 95 * S  # transition platform

            elif section == 2:
                # Pendulum bridge: stay centered, run forward
                target_x = 0
                target_z = min(pz + 12 * S, 185 * S)

                # Jump periodically to dodge pendulums
                if now - last_jump > 1.5:
                    send_input(sess, "Jump")
                    last_jump = now

            elif section == 3:
                # Hex-a-gone: move forward steadily
                target_x = 0
                target_z = min(pz + 8 * S, FINISH_Z)

            # Send move command
            send_input(sess, "MoveTo", {"position": [target_x, py, target_z]})

            time.sleep(args.tick)

        if not finished:
            print("\n\nDuration reached without finishing.")

    except KeyboardInterrupt:
        print("\n\nInterrupted.")
    finally:
        leave(sess)

    return 0


if __name__ == "__main__":
    sys.exit(main())
