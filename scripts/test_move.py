#!/usr/bin/env python3
"""
Test script to verify MoveTo works correctly.
Joins arena, sends one MoveTo, and checks if agent reaches target.
"""

import argparse
import os
from pathlib import Path
import sys
import time

from dotenv import load_dotenv
import requests

load_dotenv(Path(__file__).parent / ".env")

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")


def distance(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def distance_xz(a: list, b: list) -> float:
    """2D horizontal distance (ignores Y)"""
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def main():
    parser = argparse.ArgumentParser(description="Test MoveTo action")
    parser.add_argument("--api-key", default=os.getenv("CLAWBLOX_API_KEY"))
    parser.add_argument("--timeout", type=float, default=10.0, help="Max seconds to wait")
    parser.add_argument("--threshold", type=float, default=2.0, help="Distance to consider reached")
    parser.add_argument("--continuous", action="store_true", help="Re-send MoveTo each poll")
    args = parser.parse_args()

    if not args.api_key:
        print("Error: No API key. Set CLAWBLOX_API_KEY or use --api-key")
        sys.exit(1)

    headers = {"Authorization": f"Bearer {args.api_key}"}

    # Find arena game
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    games = resp.json().get("games", [])
    arena = next((g for g in games if "arena" in g.get("name", "").lower()), games[0] if games else None)
    if not arena:
        print("No games found")
        sys.exit(1)

    game_id = arena["id"]
    print(f"Game: {arena['name']} ({game_id})")

    # Leave all games first
    for g in games:
        requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers)

    # Join
    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers)
    if resp.status_code != 200:
        print(f"Failed to join: {resp.text}")
        sys.exit(1)
    print("Joined game")

    try:
        # Get initial position
        resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
        obs = resp.json()
        start_pos = obs["player"]["position"]
        print(f"Start position: {start_pos}")

        # Pick target: 10 units in +X direction
        target = [start_pos[0] + 10, start_pos[1], start_pos[2]]
        print(f"Target position: {target}")

        # Send MoveTo
        payload = {"type": "MoveTo", "data": {"position": target}}
        print(f"Sending MoveTo: {payload}")
        resp = requests.post(f"{API_BASE}/games/{game_id}/input", headers=headers, json=payload)
        print(f"MoveTo response: {resp.status_code} {resp.text}")

        # Poll until reached or timeout
        start_time = time.time()
        last_pos = start_pos[:]
        while time.time() - start_time < args.timeout:
            time.sleep(0.2)

            # Re-send MoveTo each iteration to test if continuous commands work
            if args.continuous:
                resp = requests.post(f"{API_BASE}/games/{game_id}/input", headers=headers, json=payload)

            resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
            obs = resp.json()
            pos = obs["player"]["position"]
            dist_to_target_xz = distance_xz(pos, target)  # 2D horizontal (engine uses this)
            dist_moved = distance(pos, last_pos)
            vel = dist_moved / 0.2

            print(f"Pos: ({pos[0]:6.2f}, {pos[1]:5.2f}, {pos[2]:6.2f}) | "
                  f"Dist XZ: {dist_to_target_xz:5.2f} | Vel: {vel:5.2f}")

            if dist_to_target_xz < args.threshold:
                print(f"\n[SUCCESS] Reached target in {time.time() - start_time:.1f}s")
                return

            last_pos = pos[:]

        total_moved = distance(obs["player"]["position"], start_pos)
        print(f"\n[FAIL] Timeout. Total distance moved: {total_moved:.2f}")
        if total_moved < 0.5:
            print("[DIAGNOSIS] Agent did not move at all - MoveTo may not be working")
        else:
            print("[DIAGNOSIS] Agent moved but didn't reach target - pathfinding issue?")

    finally:
        requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers)
        print("Left game")


if __name__ == "__main__":
    main()
