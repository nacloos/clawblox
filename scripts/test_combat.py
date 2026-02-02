#!/usr/bin/env python3
"""
Test combat: one agent stands still, another shoots at it.
Verifies damage and death mechanics.
"""

import json
import os
from pathlib import Path
import random
import sys
import time

import requests

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
KEYS_CACHE = Path("/tmp/clawblox_agent_keys.json")


def load_cached_keys() -> list[str]:
    if KEYS_CACHE.exists():
        try:
            return json.loads(KEYS_CACHE.read_text()).get("keys", [])
        except:
            pass
    return []


def save_cached_keys(keys: list[str]):
    KEYS_CACHE.write_text(json.dumps({"keys": keys}))


def register_agent(name: str) -> str | None:
    try:
        resp = requests.post(
            f"{API_BASE}/agents/register",
            json={"name": name, "description": "Test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def get_api_keys(num: int) -> list[str]:
    keys = []
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        keys.append(env_key)
    for k in load_cached_keys():
        if k not in keys:
            keys.append(k)
    while len(keys) < num:
        name = f"combat_test_{random.randint(1000, 9999)}"
        print(f"Registering {name}...")
        key = register_agent(name)
        if key:
            keys.append(key)
    save_cached_keys(keys)
    return keys[:num]


def main():
    print(f"API: {API_BASE}")

    # Get 2 API keys
    keys = get_api_keys(2)
    shooter_key = keys[0]
    target_key = keys[1]

    shooter_headers = {"Authorization": f"Bearer {shooter_key}"}
    target_headers = {"Authorization": f"Bearer {target_key}"}

    # Find game
    resp = requests.get(f"{API_BASE}/games", headers=shooter_headers)
    games = resp.json().get("games", [])
    if not games:
        print("No games")
        sys.exit(1)

    game_id = games[0]["id"]
    print(f"Game: {games[0]['name']}")

    # Leave any existing
    for h in [shooter_headers, target_headers]:
        for g in games:
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=h)

    # Join both
    requests.post(f"{API_BASE}/games/{game_id}/join", headers=target_headers)
    print("Target joined")
    time.sleep(0.3)

    requests.post(f"{API_BASE}/games/{game_id}/join", headers=shooter_headers)
    print("Shooter joined")
    time.sleep(0.5)

    # Get positions
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=shooter_headers)
    shooter_obs = resp.json()
    shooter_pos = shooter_obs["player"]["position"]
    print(f"Shooter at: {shooter_pos}")

    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=target_headers)
    target_obs = resp.json()
    target_pos = target_obs["player"]["position"]
    target_health = target_obs["player"]["health"]
    print(f"Target at: {target_pos}, health: {target_health}")

    # Move shooter toward target
    print("\nMoving shooter toward target...")
    for i in range(30):
        # Move toward target
        move_payload = {"type": "MoveTo", "data": {"position": target_pos}}
        requests.post(f"{API_BASE}/games/{game_id}/input", headers=shooter_headers, json=move_payload, timeout=5)

        resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=shooter_headers, timeout=5)
        obs = resp.json()
        shooter_pos = obs["player"]["position"]

        dist = ((shooter_pos[0] - target_pos[0])**2 + (shooter_pos[2] - target_pos[2])**2)**0.5
        if dist < 20:
            print(f"In range (dist={dist:.1f}), starting to shoot")
            break

        time.sleep(0.1)

    # Check game state
    print(f"\nGame status: {shooter_obs.get('game_status', 'unknown')}")

    # Get fresh target position
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=target_headers, timeout=5)
    target_obs = resp.json()
    target_pos = target_obs["player"]["position"]
    print(f"Fresh target pos: {target_pos}")

    # Shoot at target
    print("\nShooting at target...")
    for i in range(30):
        # Get fresh positions each time
        resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=shooter_headers, timeout=5)
        shooter_obs = resp.json()
        shooter_pos = shooter_obs["player"]["position"]

        resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=target_headers, timeout=5)
        target_obs = resp.json()
        target_pos = target_obs["player"]["position"]
        new_health = target_obs["player"]["health"]

        # Fire at current target position
        fire_payload = {"type": "Fire", "data": {"target": target_pos}}
        resp = requests.post(f"{API_BASE}/games/{game_id}/input", headers=shooter_headers, json=fire_payload, timeout=5)

        dist = ((shooter_pos[0] - target_pos[0])**2 + (shooter_pos[2] - target_pos[2])**2)**0.5
        print(f"Shot {i+1}: shooter=({shooter_pos[0]:.1f},{shooter_pos[2]:.1f}) target=({target_pos[0]:.1f},{target_pos[2]:.1f}) dist={dist:.1f} hp={new_health}")

        if new_health != target_health:
            print(f"  [DAMAGE] {target_health} -> {new_health}")
            target_health = new_health

        if target_health <= 0:
            print("[KILL] Target is dead!")
            break

        time.sleep(0.35)  # Wait longer than pistol fire rate (0.3s)

    # Cleanup
    requests.post(f"{API_BASE}/games/{game_id}/leave", headers=shooter_headers)
    requests.post(f"{API_BASE}/games/{game_id}/leave", headers=target_headers)
    print("\nDone")


if __name__ == "__main__":
    main()
