#!/usr/bin/env python3
"""
Simple movement test - just move back and forth to debug sticking.
"""

import json
import os
import random
import sys
import time
from pathlib import Path

import requests

# Load .env
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
GAME_ID = "26c869ee-da7b-48a4-a198-3daa870ef652"
KEYS_CACHE = Path("/tmp/clawblox_test_keys.json")


def get_api_key():
    if k := os.getenv("CLAWBLOX_API_KEY"):
        return k
    if KEYS_CACHE.exists():
        keys = json.loads(KEYS_CACHE.read_text()).get("keys", [])
        if keys:
            return keys[0]
    resp = requests.post(f"{API_BASE}/agents/register",
                         json={"name": f"test_{random.randint(1000,9999)}", "description": "test"}, timeout=10)
    key = resp.json()["agent"]["api_key"]
    KEYS_CACHE.write_text(json.dumps({"keys": [key]}))
    return key


def main():
    api_key = get_api_key()
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave existing games
    try:
        for g in requests.get(f"{API_BASE}/games", headers=headers, timeout=5).json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join
    print(f"Joining {GAME_ID}...")
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Failed: {resp.text}")
        return 1
    print("Joined!")
    time.sleep(1.0)

    def get_pos():
        r = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        if r.status_code != 200:
            print(f"Observe failed: {r.status_code} {r.text}")
            return None
        return r.json()["player"]["position"]

    def move_to(x, y, z):
        r = requests.post(f"{API_BASE}/games/{GAME_ID}/input", headers=headers,
                          json={"type": "MoveTo", "data": {"position": [x, y, z]}}, timeout=5)
        return r.status_code == 200

    # Get initial position
    pos = get_pos()
    if not pos:
        return 1
    print(f"Start: ({pos[0]:.1f}, {pos[1]:.1f}, {pos[2]:.1f})")

    # Test 1: move -X (West) 10 times FIRST
    print("\n=== Moving -X (West) 10 times (FIRST) ===")
    for i in range(10):
        pos = get_pos()
        if not pos:
            break
        target_x = pos[0] - 10
        print(f"  [{i+1}] pos=({pos[0]:.1f}, {pos[2]:.1f}) -> target_x={target_x:.1f}", end="")
        move_to(target_x, pos[1], pos[2])
        time.sleep(1.0)  # Longer wait to let physics settle
        new_pos = get_pos()
        if new_pos:
            dx = new_pos[0] - pos[0]
            print(f" => moved dx={dx:+.1f}")
        else:
            print(" => observe failed")

    # Test 2: move +X (East) 10 times SECOND
    print("\n=== Moving +X (East) 10 times (SECOND) ===")
    for i in range(10):
        pos = get_pos()
        if not pos:
            break
        target_x = pos[0] + 10
        print(f"  [{i+1}] pos=({pos[0]:.1f}, {pos[2]:.1f}) -> target_x={target_x:.1f}", end="")
        move_to(target_x, pos[1], pos[2])
        time.sleep(1.0)  # Longer wait to let physics settle
        new_pos = get_pos()
        if new_pos:
            dx = new_pos[0] - pos[0]
            print(f" => moved dx={dx:+.1f}")
        else:
            print(" => observe failed")

    # Leave
    requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
    print("\nDone.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
