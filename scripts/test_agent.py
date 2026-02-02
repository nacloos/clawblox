#!/usr/bin/env python3
"""
Test agent that joins the arena game and moves to random positions.

Usage:
    uv run scripts/test_agent.py [--api-key YOUR_KEY] [--register]

If --register is provided, registers a new agent first.
Otherwise, requires an API key via --api-key, CLAWBLOX_API_KEY env var, or .env file.
"""

import argparse
import os
from pathlib import Path
import random
import sys
import time

from dotenv import load_dotenv
import requests

# Load .env from script directory
load_dotenv(Path(__file__).parent / ".env")

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")


def register_agent(name: str = "TestAgent") -> str:
    """Register a new agent and return the API key."""
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": name, "description": "Test agent that moves randomly"},
    )
    resp.raise_for_status()
    data = resp.json()
    api_key = data.get("agent", {}).get("api_key") or data.get("api_key")
    print(f"Registered agent: {name}")
    print(f"API Key: {api_key}")
    return api_key


def list_games(headers: dict) -> list:
    """List all available games."""
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    return resp.json().get("games", [])


def find_arena_game(games: list) -> dict | None:
    """Find an arena game from the list."""
    for game in games:
        name = game.get("name", "").lower()
        game_type = game.get("game_type", "").lower()
        if "arena" in name or "arena" in game_type or "arsenal" in name:
            return game
    return None


def join_game(game_id: str, headers: dict) -> bool:
    """Join a game."""
    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers)
    if resp.status_code == 200:
        print(f"Joined game: {game_id}")
        return True
    print(f"Failed to join game: {resp.text}")
    return False


def leave_game(game_id: str, headers: dict, quiet: bool = False):
    """Leave a game."""
    resp = requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers)
    if resp.status_code == 200:
        if not quiet:
            print(f"Left game: {game_id}")
    else:
        if not quiet:
            print(f"Failed to leave game: {resp.text}")


def leave_all_games(headers: dict):
    """Leave all games the agent is currently in."""
    games = list_games(headers)
    for game in games:
        leave_game(game["id"], headers, quiet=True)
    print("Left all games")


def observe(game_id: str, headers: dict) -> dict:
    """Get current game observation."""
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
    if resp.status_code != 200:
        raise RuntimeError(f"Observe failed: {resp.status_code} {resp.text}")
    return resp.json()


def move_to(game_id: str, position: list, headers: dict):
    """Send MoveTo input."""
    resp = requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": position}},
    )
    if resp.status_code != 200:
        raise RuntimeError(f"MoveTo failed: {resp.status_code} {resp.text}")
    return True


def shoot(game_id: str, target: list, headers: dict):
    """Send Fire input with target position."""
    resp = requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json={"type": "Fire", "data": {"target": target}},
    )
    if resp.status_code != 200:
        raise RuntimeError(f"Fire failed: {resp.status_code} {resp.text}")
    return True


def normalize(v: list) -> list:
    """Normalize a vector."""
    mag = (v[0] ** 2 + v[1] ** 2 + v[2] ** 2) ** 0.5
    if mag == 0:
        return [0, 0, 0]
    return [v[0] / mag, v[1] / mag, v[2] / mag]


def distance(a: list, b: list) -> float:
    """Calculate distance between two positions."""
    return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def direction_to(from_pos: list, to_pos: list) -> list:
    """Get normalized direction from one position to another."""
    d = [to_pos[0] - from_pos[0], to_pos[1] - from_pos[1], to_pos[2] - from_pos[2]]
    return normalize(d)


def random_position(center: list = None, radius: float = 20.0) -> list:
    """Generate a random position within radius of center."""
    if center is None:
        center = [0.0, 1.0, 0.0]
    x = center[0] + random.uniform(-radius, radius)
    z = center[2] + random.uniform(-radius, radius)
    y = center[1]  # Keep same height
    return [x, y, z]


def run_agent(api_key: str):
    """Main agent loop."""
    headers = {"Authorization": f"Bearer {api_key}"}

    # List games
    print("\nFetching available games...")
    games = list_games(headers)
    if not games:
        print("No games available. Make sure the server is running and has games.")
        return

    print(f"Found {len(games)} game(s):")
    for g in games:
        status = g.get("status", "unknown")
        players = g.get("player_count", 0)
        print(f"  - {g['name']} ({g['game_type']}) - {status}, {players} players")

    # Find arena game
    arena = find_arena_game(games)
    if not arena:
        print("\nNo arena game found. Using first available game.")
        arena = games[0]

    game_id = arena["id"]
    print(f"\nSelected game: {arena['name']} (id: {game_id})")

    # Leave any existing game first, then join
    leave_all_games(headers)
    if not join_game(game_id, headers):
        return

    last_shoot_time = 0
    shoot_interval = 0.2  # Shoot every 200ms when enemy visible
    current_target = None
    target_reached_threshold = 1.5  # Consider target reached within this distance
    last_position = None
    stuck_start_time = None
    stuck_threshold = 0.5  # Consider stuck if moved less than this
    stuck_timeout = 2.0  # Pick new target if stuck for this long

    try:
        print("\nStarting agent loop (Ctrl+C to stop)...\n")
        while True:
            try:
                obs = observe(game_id, headers)
            except RuntimeError as e:
                print(f"\nObservation error: {e}, retrying...")
                time.sleep(1)
                continue

            game_status = obs.get("game_status", "unknown")
            if game_status == "finished":
                print("Game finished!")
                break

            player = obs.get("player", {})
            position = player.get("position", [0, 0, 0])
            health = player.get("health", 0)
            tick = obs.get("tick", 0)
            attrs = player.get("attributes", {})
            other_players = obs.get("other_players", [])

            # Print status
            weapon = attrs.get("WeaponName", "Unknown")
            kills = attrs.get("Kills", 0)
            deaths = attrs.get("Deaths", 0)
            enemies = len(other_players)
            target_dist = distance(position, current_target) if current_target else 0
            print(
                f"Tick {tick:5d} | Pos: ({position[0]:6.1f}, {position[1]:5.1f}, {position[2]:6.1f}) | "
                f"HP: {health:3d} | {weapon:12s} | K/D: {kills}/{deaths} | Enemies: {enemies} | Target: {target_dist:.1f}",
                end="\r",
            )

            now = time.time()

            # Find nearest enemy
            nearest_enemy = None
            nearest_dist = float("inf")
            for enemy in other_players:
                enemy_pos = enemy.get("position", [0, 0, 0])
                dist = distance(position, enemy_pos)
                if dist < nearest_dist:
                    nearest_dist = dist
                    nearest_enemy = enemy

            # Shoot at nearest enemy
            if nearest_enemy and now - last_shoot_time >= shoot_interval:
                enemy_pos = nearest_enemy.get("position", [0, 0, 0])
                shoot(game_id, enemy_pos, headers)
                last_shoot_time = now

                # Chase enemy if far
                if nearest_dist > 10:
                    current_target = enemy_pos
                    move_to(game_id, current_target, headers)

            # Shoot randomly when no enemies (less frequently)
            elif now - last_shoot_time >= 1.0:
                # Pick a random target position
                random_target = [
                    position[0] + random.uniform(-30, 30),
                    position[1] + random.uniform(-2, 2),
                    position[2] + random.uniform(-30, 30),
                ]
                shoot(game_id, random_target, headers)
                last_shoot_time = now

            # Check if stuck (not making progress toward target)
            now = time.time()
            if last_position is not None and current_target is not None:
                moved = distance(position, last_position)
                if moved < stuck_threshold * 0.1:  # Barely moved this tick
                    if stuck_start_time is None:
                        stuck_start_time = now
                    elif now - stuck_start_time > stuck_timeout:
                        print(f"\n[STUCK] No progress for {stuck_timeout}s, picking new target")
                        current_target = None
                        stuck_start_time = None
                else:
                    stuck_start_time = None  # Reset if making progress
            last_position = position[:]

            # Check if we need a new target (no target, reached it, or stuck)
            need_new_target = (
                current_target is None or
                distance(position, current_target) < target_reached_threshold
            )

            if need_new_target and not nearest_enemy:
                # Pick a new random target
                current_target = random_position(center=[0, position[1], 0], radius=25.0)
                move_to(game_id, current_target, headers)
                stuck_start_time = None

            # 10 Hz loop
            time.sleep(0.1)

    except KeyboardInterrupt:
        print("\n\nStopping agent...")
    finally:
        leave_game(game_id, headers)


def main():
    parser = argparse.ArgumentParser(description="Test agent for Clawblox arena game")
    parser.add_argument("--api-key", help="API key for authentication")
    parser.add_argument(
        "--register", action="store_true", help="Register a new agent first"
    )
    parser.add_argument(
        "--leave", action="store_true", help="Leave all games and exit"
    )
    args = parser.parse_args()

    api_key = args.api_key or os.getenv("CLAWBLOX_API_KEY")

    if args.register:
        api_key = register_agent()
        print(f"\nSet this env var to reuse: export CLAWBLOX_API_KEY={api_key}\n")
    elif not api_key:
        print("Error: No API key provided.")
        print("Use --api-key YOUR_KEY or set CLAWBLOX_API_KEY env var")
        print("Or use --register to create a new agent")
        sys.exit(1)

    if args.leave:
        headers = {"Authorization": f"Bearer {api_key}"}
        leave_all_games(headers)
        return

    run_agent(api_key)


if __name__ == "__main__":
    main()
