#!/usr/bin/env python3
"""Minimal test: join one player and move to a single target position.

Usage:
  uv run games/fps-arena/test_move.py --api-base http://localhost:8080/api/v1 --game-id <uuid>
  uv run games/fps-arena/test_move.py --api-base http://localhost:8080
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any

import requests

AGENT_NAME = "fps-move-test"
TARGET = [24.0, 2.0, 0.0]


def key_cache_path() -> Path:
    return Path.home() / ".clawblox" / "fps_arena_test_player_keys.json"


def load_cached_api_key(api_base: str, name: str) -> str | None:
    path = key_cache_path()
    try:
        data = json.loads(path.read_text())
    except Exception:
        return None
    key = data.get(f"{api_base}::{name}")
    return key if isinstance(key, str) and key else None


def save_cached_api_key(api_base: str, name: str, api_key: str) -> None:
    path = key_cache_path()
    data: dict[str, str] = {}
    try:
        data = json.loads(path.read_text())
        if not isinstance(data, dict):
            data = {}
    except Exception:
        data = {}
    data[f"{api_base}::{name}"] = api_key
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True))


def register_agent(api_base: str, name: str) -> str:
    r = requests.post(
        f"{api_base}/agents/register",
        json={"name": name, "description": "Move-to-target test bot"},
        timeout=10,
    )
    r.raise_for_status()
    body = r.json()
    key = body.get("api_key")
    if not key:
        agent = body.get("agent")
        if isinstance(agent, dict):
            key = agent.get("api_key")
    if not isinstance(key, str) or not key:
        raise RuntimeError("register response missing api_key")
    return key


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Move-to-target test")
    p.add_argument("--api-base", default="http://localhost:8080")
    p.add_argument("--game-id", default=None)
    p.add_argument("--api-key", default=None)
    p.add_argument("--target", type=float, nargs=3, default=TARGET, metavar=("X", "Y", "Z"))
    p.add_argument("--duration", type=float, default=30.0)
    p.add_argument("--tick", type=float, default=0.5)
    return p.parse_args()


def main() -> int:
    args = parse_args()
    api_base = args.api_base.rstrip("/")
    is_platform = api_base.endswith("/api/v1")
    target = args.target

    # Auth
    if is_platform:
        if not args.game_id:
            print("[fatal] --game-id required for /api/v1 mode")
            return 1
        api_key = args.api_key or load_cached_api_key(api_base, AGENT_NAME)
        if api_key:
            print(f"[auth] using cached key")
        else:
            api_key = register_agent(api_base, AGENT_NAME)
            save_cached_api_key(api_base, AGENT_NAME, api_key)
            print(f"[auth] registered and cached key")
        headers = {"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"}
    else:
        headers = {"Content-Type": "application/json"}

    # Join
    if is_platform:
        r = requests.post(f"{api_base}/games/{args.game_id}/join", headers=headers, timeout=10)
    else:
        r = requests.post(f"{api_base}/join", params={"name": AGENT_NAME}, timeout=10)
        body = r.json()
        headers["X-Session"] = body["session"]
    r.raise_for_status()
    print("[join] ok")

    def observe() -> dict[str, Any]:
        if is_platform:
            resp = requests.get(f"{api_base}/games/{args.game_id}/observe", headers=headers, timeout=10)
        else:
            resp = requests.get(f"{api_base}/observe", headers=headers, timeout=10)
        resp.raise_for_status()
        return resp.json()

    def send_input(input_type: str, data: dict[str, Any]) -> None:
        payload = {"type": input_type, "data": data}
        if is_platform:
            resp = requests.post(f"{api_base}/games/{args.game_id}/input", headers=headers, json=payload, timeout=10)
        else:
            resp = requests.post(f"{api_base}/input", headers=headers, json=payload, timeout=10)
        resp.raise_for_status()

    print(f"[target] ({target[0]:.1f}, {target[1]:.1f}, {target[2]:.1f})")

    # Send MoveTo once, then just observe
    send_input("MoveTo", {"position": target})
    print("[move] sent MoveTo")

    start = time.time()
    try:
        while time.time() - start < args.duration:
            obs = observe()
            player = obs.get("player") or {}
            pos = player.get("position") or [0, 0, 0]
            tick = obs.get("tick")
            hp = player.get("health")
            dx = pos[0] - target[0]
            dy = pos[1] - target[1]
            dz = pos[2] - target[2]
            dist = (dx * dx + dy * dy + dz * dz) ** 0.5
            print(f"[obs] tick={tick} pos=({pos[0]:.1f},{pos[1]:.1f},{pos[2]:.1f}) dist={dist:.2f} hp={hp}")
            time.sleep(args.tick)
    except KeyboardInterrupt:
        print("[done] interrupted")

    # Leave
    try:
        if is_platform:
            requests.post(f"{api_base}/games/{args.game_id}/leave", headers=headers, timeout=5)
        print("[leave] ok")
    except Exception:
        pass

    return 0


if __name__ == "__main__":
    sys.exit(main())
