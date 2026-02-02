#!/usr/bin/env python3
"""
Simple exploration agent that continuously moves around the arena.
Picks random waypoints and navigates to them, with stuck detection.
Supports running multiple agents in parallel with auto-registration.
"""

import argparse
import json
import os
from pathlib import Path
import random
import sys
import threading
import time

from dotenv import load_dotenv
import requests

load_dotenv(Path(__file__).parent.parent.parent / ".env")

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
KEYS_CACHE = Path("/tmp/clawblox_agent_keys.json")


def load_cached_keys() -> list[str]:
    """Load API keys from cache file"""
    if KEYS_CACHE.exists():
        try:
            data = json.loads(KEYS_CACHE.read_text())
            return data.get("keys", [])
        except (json.JSONDecodeError, KeyError):
            pass
    return []


def save_cached_keys(keys: list[str]):
    """Save API keys to cache file"""
    KEYS_CACHE.write_text(json.dumps({"keys": keys}, indent=2))


def register_agent(name: str) -> str | None:
    """Register a new agent and return its API key"""
    try:
        resp = requests.post(
            f"{API_BASE}/agents/register",
            json={"name": name, "description": "Exploration test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            data = resp.json()
            return data["agent"]["api_key"]
        elif resp.status_code == 409:  # Name taken
            return None
        else:
            print(f"Registration failed: {resp.status_code} {resp.text}")
            return None
    except Exception as e:
        print(f"Registration error: {e}")
        return None


def ensure_api_keys(num_needed: int) -> list[str]:
    """Ensure we have enough API keys, registering new agents if needed"""
    # Start with env keys
    env_keys = os.getenv("CLAWBLOX_API_KEYS") or os.getenv("CLAWBLOX_API_KEY") or ""
    keys = [k.strip() for k in env_keys.split(",") if k.strip()]

    # Add cached keys
    cached = load_cached_keys()
    for k in cached:
        if k not in keys:
            keys.append(k)

    # Register more if needed
    while len(keys) < num_needed:
        name = f"explorer_{random.randint(1000, 9999)}"
        print(f"Registering new agent: {name}")
        key = register_agent(name)
        if key:
            keys.append(key)
            print(f"  Got key: {key[:20]}...")
        else:
            # Try different name
            continue

    # Cache all keys
    save_cached_keys(keys)
    return keys

# Arena bounds (200x200 arena, stay slightly inside walls)
ARENA_MIN = -90
ARENA_MAX = 90
WAYPOINT_Y = 3  # Ground level


def distance_xz(a: list, b: list) -> float:
    """2D horizontal distance (ignores Y)"""
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def random_waypoint() -> list:
    """Pick a random point in the arena"""
    x = random.uniform(ARENA_MIN, ARENA_MAX)
    z = random.uniform(ARENA_MIN, ARENA_MAX)
    return [x, WAYPOINT_Y, z]


def run_agent(agent_id: int, api_key: str, game_id: str, args, stop_event: threading.Event):
    """Run a single exploration agent"""
    headers = {"Authorization": f"Bearer {api_key}"}
    prefix = f"[Agent {agent_id}]"

    # Leave all games first, then join
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    if resp.status_code == 200:
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers)

    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers)
    if resp.status_code != 200:
        print(f"{prefix} Failed to join: {resp.text}")
        return
    print(f"{prefix} Joined game")

    # Exploration state
    waypoint = None
    last_pos = None
    stuck_start = None
    waypoints_reached = 0

    try:
        while not stop_event.is_set():
            time.sleep(0.2)

            # Get observation
            resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
            if resp.status_code != 200:
                time.sleep(1)
                continue

            obs = resp.json()
            pos = obs["player"]["position"]

            # Pick initial waypoint
            if waypoint is None:
                waypoint = random_waypoint()
                print(f"{prefix} New waypoint: ({waypoint[0]:6.1f}, {waypoint[2]:6.1f})")

            dist_to_waypoint = distance_xz(pos, waypoint)

            # Check if arrived
            if dist_to_waypoint < args.arrival_threshold:
                waypoints_reached += 1
                print(f"{prefix} [ARRIVED #{waypoints_reached}] at ({pos[0]:6.1f}, {pos[2]:6.1f})")
                waypoint = random_waypoint()
                print(f"{prefix} New waypoint: ({waypoint[0]:6.1f}, {waypoint[2]:6.1f})")
                stuck_start = None
                last_pos = pos
                continue

            # Check if stuck
            if last_pos is not None:
                moved = distance_xz(pos, last_pos)
                if moved < args.stuck_threshold * 0.2:
                    if stuck_start is None:
                        stuck_start = time.time()
                    elif time.time() - stuck_start > args.stuck_time:
                        print(f"{prefix} [STUCK] picking new waypoint")
                        waypoint = random_waypoint()
                        print(f"{prefix} New waypoint: ({waypoint[0]:6.1f}, {waypoint[2]:6.1f})")
                        stuck_start = None
                else:
                    stuck_start = None

            # Send MoveTo
            payload = {"type": "MoveTo", "data": {"position": waypoint}}
            requests.post(f"{API_BASE}/games/{game_id}/input", headers=headers, json=payload)

            last_pos = pos

    finally:
        requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers)
        print(f"{prefix} Left game. Waypoints reached: {waypoints_reached}")


def main():
    parser = argparse.ArgumentParser(description="Exploration agent(s)")
    parser.add_argument("--api-keys", help="Comma-separated API keys (or set CLAWBLOX_API_KEYS)")
    parser.add_argument("--num-agents", "-n", type=int, default=1, help="Number of agents to run")
    parser.add_argument("--arrival-threshold", type=float, default=3.0)
    parser.add_argument("--stuck-threshold", type=float, default=1.0)
    parser.add_argument("--stuck-time", type=float, default=2.0)
    parser.add_argument("--duration", "-d", type=float, default=None, help="Run for N seconds then stop")
    args = parser.parse_args()

    # Gather API keys (auto-register if needed)
    if args.api_keys:
        api_keys = [k.strip() for k in args.api_keys.split(",") if k.strip()]
    else:
        api_keys = ensure_api_keys(args.num_agents)

    if not api_keys:
        print("Error: No API keys available and registration failed")
        sys.exit(1)

    print(f"Using {len(api_keys)} API key(s) for {args.num_agents} agent(s)")

    # Find arena game
    headers = {"Authorization": f"Bearer {api_keys[0]}"}
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    games = resp.json().get("games", [])
    arena = next((g for g in games if "arena" in g.get("name", "").lower()), games[0] if games else None)
    if not arena:
        print("No games found")
        sys.exit(1)

    game_id = arena["id"]
    print(f"Game: {arena['name']} ({game_id})")
    print(f"Starting {args.num_agents} agent(s)...\n")

    stop_event = threading.Event()
    threads = []

    for i in range(args.num_agents):
        api_key = api_keys[i % len(api_keys)]
        t = threading.Thread(target=run_agent, args=(i, api_key, game_id, args, stop_event))
        t.daemon = True
        t.start()
        threads.append(t)
        time.sleep(0.1)  # Stagger joins

    try:
        start_time = time.time()
        while True:
            time.sleep(0.5)
            if args.duration and (time.time() - start_time) >= args.duration:
                print(f"\n\nDuration {args.duration}s reached, stopping...")
                break
    except KeyboardInterrupt:
        print("\n\nStopping agents...")
    finally:
        stop_event.set()
        for t in threads:
            t.join(timeout=2)


if __name__ == "__main__":
    main()
