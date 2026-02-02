#!/usr/bin/env python3
"""
Test agent that only shoots to debug projectile rendering.

Usage:
    uv run scripts/test_shooting.py
"""

import os
import sys
import time
from pathlib import Path

from dotenv import load_dotenv
import requests

load_dotenv(Path(__file__).parent.parent.parent / ".env")

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
API_KEY = os.getenv("CLAWBLOX_API_KEY")


def observe(game_id: str, headers: dict) -> dict:
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
    resp.raise_for_status()
    return resp.json()


def shoot(game_id: str, target: list, headers: dict):
    resp = requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json={"type": "Fire", "data": {"target": target}},
    )
    resp.raise_for_status()


def list_games(headers: dict) -> list:
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    return resp.json().get("games", [])


def join_game(game_id: str, headers: dict):
    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers)
    if resp.status_code != 200:
        print(f"Join failed (may already be in game): {resp.text}")


def leave_game(game_id: str, headers: dict):
    requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers)


def main():
    if not API_KEY:
        print("Error: Set CLAWBLOX_API_KEY in .env or environment")
        sys.exit(1)

    headers = {"Authorization": f"Bearer {API_KEY}"}

    # Find and join game
    games = list_games(headers)
    if not games:
        print("No games available")
        sys.exit(1)

    game = games[0]
    game_id = game["id"]
    print(f"Joining: {game['name']}")
    join_game(game_id, headers)

    # Target offsets to cycle through (relative to player position)
    offsets = [
        [50, 0, 0],    # +X
        [0, 0, 50],    # +Z
        [-50, 0, 0],   # -X
        [0, 0, -50],   # -Z
    ]
    offset_idx = 0

    print("\nShooting every 2 seconds... (Ctrl+C to stop)")
    print("Watch the frontend to see projectiles\n")

    try:
        while True:
            obs = observe(game_id, headers)
            pos = obs["player"]["position"]
            kills = obs["player"]["attributes"].get("Kills", 0)
            deaths = obs["player"]["attributes"].get("Deaths", 0)

            offset = offsets[offset_idx]
            target = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]]
            offset_idx = (offset_idx + 1) % len(offsets)

            print(f"Pos: ({pos[0]:6.1f}, {pos[1]:5.1f}, {pos[2]:6.1f}) | K/D: {kills}/{deaths} | Target: ({target[0]:6.1f}, {target[1]:5.1f}, {target[2]:6.1f})")

            shoot(game_id, target, headers)
            time.sleep(2.0)

    except KeyboardInterrupt:
        print("\nStopping...")
    finally:
        leave_game(game_id, headers)
        print("Left game")


if __name__ == "__main__":
    main()
