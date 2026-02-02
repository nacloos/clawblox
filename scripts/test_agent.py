#!/usr/bin/env python3
"""
Test agent focused on movement. Explores the arena by moving to random waypoints.
"""

import argparse
import json
import math
import os
from pathlib import Path
import random
import sys
import threading
import time

import requests

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
KEYS_CACHE = Path("/tmp/clawblox_agent_keys.json")

# Arena bounds (200x200, stay inside walls)
ARENA_MIN = -80
ARENA_MAX = 80


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


def get_api_keys(num_needed: int) -> list[str]:
    """Get or register enough API keys for num_needed agents"""
    # Start with env key
    keys = []
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        keys.append(env_key)

    # Add cached keys
    for k in load_cached_keys():
        if k not in keys:
            keys.append(k)

    # Register more if needed
    while len(keys) < num_needed:
        name = f"agent_{random.randint(1000, 9999)}"
        print(f"Registering {name}...", flush=True)
        key = register_agent(name)
        if key:
            keys.append(key)
            print(f"  OK: {key[:20]}...", flush=True)

    # Cache all keys
    save_cached_keys(keys)
    return keys[:num_needed]


def distance_xz(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def random_waypoint() -> list:
    return [random.uniform(ARENA_MIN, ARENA_MAX), 3.0, random.uniform(ARENA_MIN, ARENA_MAX)]


def unstuck_waypoint(pos: list, last_waypoint: list) -> list:
    """When stuck, pick a waypoint in the opposite direction from where we were heading"""
    # Direction we were trying to go
    dx = last_waypoint[0] - pos[0]
    dz = last_waypoint[2] - pos[2]

    # Go roughly opposite direction, with some randomness
    angle = random.uniform(-0.5, 0.5)  # radians of randomness
    import math
    cos_a, sin_a = math.cos(angle), math.sin(angle)

    # Rotate and reverse direction
    new_dx = -(dx * cos_a - dz * sin_a)
    new_dz = -(dx * sin_a + dz * cos_a)

    # Normalize and scale to a reasonable distance (20-40 units)
    dist = (new_dx**2 + new_dz**2) ** 0.5
    if dist > 0:
        scale = random.uniform(20, 40) / dist
        new_dx *= scale
        new_dz *= scale

    # Clamp to arena bounds
    new_x = max(ARENA_MIN, min(ARENA_MAX, pos[0] + new_dx))
    new_z = max(ARENA_MIN, min(ARENA_MAX, pos[2] + new_dz))

    return [new_x, 3.0, new_z]


def find_closest_enemy(pos: list, other_players: list) -> dict | None:
    """Find the closest visible enemy"""
    if not other_players:
        return None

    closest = None
    closest_dist = float('inf')

    for enemy in other_players:
        dist = distance_xz(pos, enemy["position"])
        if dist < closest_dist:
            closest_dist = dist
            closest = enemy

    return closest


def run_agent(agent_id: int, api_key: str, game_id: str, stop_event: threading.Event):
    """Run a single agent"""
    prefix = f"[{agent_id}]"
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join
    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"{prefix} Failed to join: {resp.text}", flush=True)
        return
    print(f"{prefix} Joined", flush=True)

    time.sleep(0.3)

    waypoint = random_waypoint()
    last_pos = None
    stuck_time = None
    arrivals = 0
    kills = 0

    try:
        tick = 0
        while not stop_event.is_set():
            # Observe
            resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers, timeout=5)
            if resp.status_code != 200:
                time.sleep(0.5)
                continue

            obs = resp.json()
            pos = obs["player"]["position"]
            other_players = obs.get("other_players", [])

            # Track kills from attributes
            player_kills = obs.get("player", {}).get("attributes", {}).get("Kills", 0)
            if player_kills > kills:
                print(f"{prefix} [KILL] total: {player_kills}", flush=True)
                kills = player_kills

            # Find closest enemy
            enemy = find_closest_enemy(pos, other_players)

            if enemy:
                enemy_pos = enemy["position"]
                enemy_dist = distance_xz(pos, enemy_pos)

                # Fire at enemy
                fire_payload = {"type": "Fire", "data": {"target": enemy_pos}}
                requests.post(f"{API_BASE}/games/{game_id}/input", headers=headers, json=fire_payload, timeout=5)

                # Move toward enemy
                waypoint = enemy_pos
                stuck_time = None

                if tick % 10 == 0:
                    print(f"{prefix} [COMBAT] enemy at dist {enemy_dist:.1f}", flush=True)
            else:
                # No enemy - explore
                dist = distance_xz(pos, waypoint)

                # Check arrival
                if dist < 5.0:
                    arrivals += 1
                    waypoint = random_waypoint()
                    stuck_time = None

                # Check stuck
                if last_pos:
                    moved = distance_xz(pos, last_pos)
                    if moved < 0.1:
                        if stuck_time is None:
                            stuck_time = time.time()
                        elif time.time() - stuck_time > 2.0:
                            waypoint = unstuck_waypoint(pos, waypoint)
                            stuck_time = None
                    else:
                        stuck_time = None

            # Send MoveTo
            move_payload = {"type": "MoveTo", "data": {"position": waypoint}}
            requests.post(f"{API_BASE}/games/{game_id}/input", headers=headers, json=move_payload, timeout=5)

            # Periodic status
            if tick % 20 == 0:
                health = obs.get("player", {}).get("health", 100)
                print(f"{prefix} pos=({pos[0]:.0f},{pos[2]:.0f}) hp={health} enemies={len(other_players)}", flush=True)

            last_pos = pos
            tick += 1
            time.sleep(0.1)

    finally:
        requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers, timeout=5)
        print(f"{prefix} Left (arrivals: {arrivals}, kills: {kills})", flush=True)


def main():
    parser = argparse.ArgumentParser(description="Test exploration agents")
    parser.add_argument("-n", "--num-agents", type=int, default=1, help="Number of agents")
    parser.add_argument("-d", "--duration", type=float, default=None, help="Run for N seconds")
    args = parser.parse_args()

    print(f"API: {API_BASE}", flush=True)

    # Get API keys
    api_keys = get_api_keys(args.num_agents)
    print(f"Got {len(api_keys)} API key(s)", flush=True)

    # Find game
    headers = {"Authorization": f"Bearer {api_keys[0]}"}
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    games = resp.json().get("games", [])
    if not games:
        print("No games available")
        sys.exit(1)

    game_id = games[0]["id"]
    print(f"Game: {games[0]['name']}", flush=True)
    print("-" * 60, flush=True)

    # Start agents
    stop_event = threading.Event()
    threads = []

    for i in range(args.num_agents):
        t = threading.Thread(target=run_agent, args=(i, api_keys[i], game_id, stop_event))
        t.daemon = True
        t.start()
        threads.append(t)
        time.sleep(0.2)  # Stagger joins

    # Run until duration or Ctrl+C
    try:
        start = time.time()
        while True:
            time.sleep(0.5)
            if args.duration and (time.time() - start) >= args.duration:
                print(f"\nDuration {args.duration}s reached", flush=True)
                break
    except KeyboardInterrupt:
        print("\nStopping...", flush=True)
    finally:
        stop_event.set()
        for t in threads:
            t.join(timeout=2)


if __name__ == "__main__":
    main()
