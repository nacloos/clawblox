#!/usr/bin/env python3
"""
Test multiple agents fighting in the arena.

Usage:
    uv run scripts/test_multi_agent.py --agents 3
"""

import argparse
import os
import sys
import time
import random
import threading
from pathlib import Path

from dotenv import load_dotenv
import requests

load_dotenv(Path(__file__).parent.parent.parent / ".env")

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")


def register_agent(name: str) -> str:
    """Register a new agent and return the API key."""
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": name, "description": f"Test agent {name}"},
    )
    if resp.status_code != 200:
        raise RuntimeError(f"Failed to register agent: {resp.text}")
    return resp.json()["agent"]["api_key"]


def get_headers(api_key: str) -> dict:
    return {"Authorization": f"Bearer {api_key}"}


def list_games(headers: dict) -> list:
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    return resp.json().get("games", [])


def join_game(game_id: str, headers: dict) -> bool:
    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers)
    return resp.status_code == 200


def leave_game(game_id: str, headers: dict):
    requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers)


def observe(game_id: str, headers: dict) -> dict | None:
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
    if resp.status_code != 200:
        return None
    return resp.json()


def move_to(game_id: str, position: list, headers: dict):
    requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": position}},
    )


def shoot(game_id: str, target: list, headers: dict):
    requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json={"type": "Fire", "data": {"target": target}},
    )


def distance(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def random_arena_position() -> list:
    """Random position within the arena bounds."""
    return [
        random.uniform(-30, 30),
        3.0,
        random.uniform(-30, 30),
    ]


def agent_loop(agent_id: int, api_key: str, game_id: str, stop_event: threading.Event):
    """Main loop for a single agent."""
    headers = get_headers(api_key)
    name = f"Agent-{agent_id}"

    # Join game
    if not join_game(game_id, headers):
        print(f"[{name}] Failed to join game")
        return

    print(f"[{name}] Joined game")

    last_shoot_time = 0
    shoot_interval = 0.3
    current_target = None
    last_status_time = 0

    try:
        while not stop_event.is_set():
            obs = observe(game_id, headers)
            if not obs or "player" not in obs:
                time.sleep(0.5)
                continue

            position = obs["player"]["position"]
            health = obs["player"].get("health", 100)
            weapon = obs["player"].get("attributes", {}).get("WeaponName", "?")
            kills = obs["player"].get("attributes", {}).get("Kills", 0)
            deaths = obs["player"].get("attributes", {}).get("Deaths", 0)

            game_status = obs.get("game_status", "unknown")

            # Print status every 5 seconds
            now = time.time()
            if now - last_status_time >= 5.0:
                enemies = obs.get("visible_players", [])
                print(f"[{name}] HP:{health:3.0f} | {weapon:15s} | K/D: {kills}/{deaths} | status:{game_status} | enemies:{len(enemies)}")
                last_status_time = now

            # Find enemies
            enemies = obs.get("visible_players", [])
            nearest_enemy = None
            nearest_dist = float("inf")

            for enemy in enemies:
                enemy_pos = enemy.get("position", [0, 0, 0])
                dist = distance(position, enemy_pos)
                if dist < nearest_dist:
                    nearest_dist = dist
                    nearest_enemy = enemy

            # Combat logic
            if nearest_enemy:
                enemy_pos = nearest_enemy["position"]

                # Shoot at enemy
                if now - last_shoot_time >= shoot_interval:
                    shoot(game_id, enemy_pos, headers)
                    last_shoot_time = now

                # Move toward or strafe around enemy
                if nearest_dist > 15:
                    # Move closer
                    move_to(game_id, enemy_pos, headers)
                elif nearest_dist < 5:
                    # Too close, back up
                    away = [
                        position[0] + (position[0] - enemy_pos[0]),
                        position[1],
                        position[2] + (position[2] - enemy_pos[2]),
                    ]
                    move_to(game_id, away, headers)
                else:
                    # Strafe
                    strafe = [
                        position[0] + random.uniform(-5, 5),
                        position[1],
                        position[2] + random.uniform(-5, 5),
                    ]
                    move_to(game_id, strafe, headers)
            else:
                # No enemies visible, roam
                if current_target is None or distance(position, current_target) < 3:
                    current_target = random_arena_position()
                move_to(game_id, current_target, headers)

            time.sleep(0.1)

    except Exception as e:
        print(f"[{name}] Error: {e}")
    finally:
        leave_game(game_id, headers)
        print(f"[{name}] Left game")


def main():
    parser = argparse.ArgumentParser(description="Test multiple agents fighting")
    parser.add_argument("--agents", type=int, default=3, help="Number of agents (default: 3)")
    parser.add_argument("--duration", type=int, default=60, help="Duration in seconds (default: 60)")
    args = parser.parse_args()

    print(f"Starting {args.agents} agents for {args.duration} seconds...")

    # Register agents
    api_keys = []
    for i in range(args.agents):
        name = f"TestAgent_{i}_{random.randint(1000, 9999)}"
        try:
            api_key = register_agent(name)
            api_keys.append(api_key)
            print(f"Registered {name}")
        except Exception as e:
            print(f"Failed to register agent {i}: {e}")
            sys.exit(1)

    # Find game
    headers = get_headers(api_keys[0])
    games = list_games(headers)
    if not games:
        print("No games available")
        sys.exit(1)

    game = games[0]
    game_id = game["id"]
    print(f"\nJoining game: {game['name']}")
    print("=" * 50)

    # Start agent threads
    stop_event = threading.Event()
    threads = []

    for i, api_key in enumerate(api_keys):
        t = threading.Thread(
            target=agent_loop,
            args=(i, api_key, game_id, stop_event),
            daemon=True,
        )
        threads.append(t)
        t.start()
        time.sleep(0.2)  # Stagger joins

    # Run for duration
    try:
        print(f"\nAgents fighting... (Ctrl+C to stop early)\n")
        time.sleep(args.duration)
    except KeyboardInterrupt:
        print("\nStopping...")

    # Stop agents
    stop_event.set()
    for t in threads:
        t.join(timeout=2.0)

    print("\nDone!")


if __name__ == "__main__":
    main()
