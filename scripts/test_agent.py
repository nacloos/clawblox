#!/usr/bin/env python3
"""
Test agent that joins the arena game and moves to random positions.

Usage:
    uv run scripts/test_agent.py [--api-key YOUR_KEY] [--register]

If --register is provided, registers a new agent first.
Otherwise, requires an API key via --api-key, CLAWBLOX_API_KEY env var, or .env file.
"""

import argparse
import asyncio
import contextlib
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


async def register_agent(name: str = "TestAgent") -> str:
    """Register a new agent and return the API key."""
    def do_request():
        return requests.post(
            f"{API_BASE}/agents/register",
            json={"name": name, "description": "Test agent that moves randomly"},
        )

    resp = await asyncio.to_thread(do_request)
    resp.raise_for_status()
    data = resp.json()
    api_key = data.get("agent", {}).get("api_key") or data.get("api_key")
    print(f"Registered agent: {name}")
    print(f"API Key: {api_key}")
    return api_key


async def list_games(headers: dict) -> list:
    """List all available games."""
    def do_request():
        return requests.get(f"{API_BASE}/games", headers=headers)

    resp = await asyncio.to_thread(do_request)
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


async def join_game(game_id: str, headers: dict) -> bool:
    """Join a game."""
    def do_request():
        return requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers)

    resp = await asyncio.to_thread(do_request)
    if resp.status_code == 200:
        print(f"Joined game: {game_id}")
        return True
    print(f"Failed to join game: {resp.text}")
    return False


async def leave_game(game_id: str, headers: dict, quiet: bool = False):
    """Leave a game."""
    def do_request():
        return requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers)

    resp = await asyncio.to_thread(do_request)
    if resp.status_code == 200:
        if not quiet:
            print(f"Left game: {game_id}")
    else:
        if not quiet:
            print(f"Failed to leave game: {resp.text}")


async def leave_all_games(headers: dict):
    """Leave all games the agent is currently in."""
    games = await list_games(headers)
    for game in games:
        await leave_game(game["id"], headers, quiet=True)
    print("Left all games")


async def observe(game_id: str, headers: dict) -> dict:
    """Get current game observation."""
    def do_request():
        return requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)

    resp = await asyncio.to_thread(do_request)
    if resp.status_code != 200:
        raise RuntimeError(f"Observe failed: {resp.status_code} {resp.text}")
    return resp.json()


async def move_to(game_id: str, position: list, headers: dict):
    """Send MoveTo input."""
    payload = {"type": "MoveTo", "data": {"position": position}}
    def do_request():
        return requests.post(
            f"{API_BASE}/games/{game_id}/input",
            headers=headers,
            json=payload,
        )

    resp = await asyncio.to_thread(do_request)
    if resp.status_code != 200:
        raise RuntimeError(f"MoveTo failed: {resp.status_code} {resp.text}")
    return True


async def shoot(game_id: str, target: list, headers: dict):
    """Send Fire input with target position."""
    def do_request():
        return requests.post(
            f"{API_BASE}/games/{game_id}/input",
            headers=headers,
            json={"type": "Fire", "data": {"target": target}},
        )

    resp = await asyncio.to_thread(do_request)
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


async def periodic_logger(state: dict, state_lock: asyncio.Lock, interval: float):
    while True:
        await asyncio.sleep(interval)
        async with state_lock:
            if not state["running"]:
                return
            tick = state["tick"]
            position = state["position"]
            target_dist = state["target_dist"]
            velocity = state["velocity"]
            start_time = state["start_time"]
        elapsed = time.monotonic() - start_time
        status_line = (
            f"T+{elapsed:6.1f}s | Tick {tick:5d} | Pos: ({position[0]:6.1f}, {position[1]:5.1f}, {position[2]:6.1f}) | "
            f"Target: {target_dist:.1f} | Vel: {velocity:.1f}"
        )
        print(status_line, flush=True)


async def run_agent(api_key: str, duration: float | None = None):
    """Main agent loop."""
    headers = {"Authorization": f"Bearer {api_key}"}
    start_time = time.monotonic()

    # List games
    print("\nFetching available games...")
    games = await list_games(headers)
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
    await leave_all_games(headers)
    if not await join_game(game_id, headers):
        return

    last_shoot_time = 0
    shoot_interval = 0.2  # Shoot every 200ms when enemy visible
    current_target = None
    target_reached_threshold = 1.5  # Consider target reached within this distance
    last_position = None
    last_position_time = None
    stuck_start_time = None
    stuck_threshold = 0.5  # Consider stuck if moved less than this
    stuck_timeout = 2.0  # Pick new target if stuck for this long
    last_move_command_time = 0.0
    move_command_interval = 0.5  # Re-issue MoveTo periodically while traveling
    last_target_distance = None
    last_progress_time = None
    progress_timeout = 2.0  # If target distance doesn't shrink for this long, re-path
    log_interval_seconds = 0.5  # Periodic position logs for movement debugging
    state_lock = asyncio.Lock()
    state = {
        "running": True,
        "start_time": start_time,
        "tick": 0,
        "position": [0.0, 0.0, 0.0],
        "target_dist": 0.0,
        "velocity": 0.0,
    }
    logger_task = asyncio.create_task(periodic_logger(state, state_lock, log_interval_seconds))

    try:
        dur_msg = f" for {duration}s" if duration else ""
        print(f"\nStarting agent loop{dur_msg} (Ctrl+C to stop)...\n")
        while True:
            # Check duration limit
            elapsed = time.monotonic() - start_time
            if duration and elapsed >= duration:
                print(f"\n[DONE] Duration limit {duration}s reached")
                break
            try:
                obs = await observe(game_id, headers)
            except RuntimeError as e:
                print(f"\nObservation error: {e}, retrying...")
                await asyncio.sleep(1)
                continue

            now = time.monotonic()
            game_status = obs.get("game_status", "unknown")
            if game_status == "finished":
                print("Game finished!")
                break

            player = obs.get("player", {})
            position = player.get("position", [0, 0, 0])
            health = player.get("health", 0)
            tick = obs.get("tick", 0)
            other_players = obs.get("other_players", [])

            # Calculate velocity for debug
            velocity = 0.0
            if last_position is not None and last_position_time is not None:
                dt = now - last_position_time
                if dt > 0:
                    velocity = distance(position, last_position) / dt

            # Update state for periodic logger
            enemies = len(other_players)
            target_dist = distance(position, current_target) if current_target else 0
            async with state_lock:
                state["tick"] = tick
                state["position"] = position[:]
                state["target_dist"] = target_dist
                state["velocity"] = velocity

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
                await shoot(game_id, enemy_pos, headers)
                last_shoot_time = now

                # Chase enemy if far
                if nearest_dist > 10:
                    current_target = enemy_pos
                    await move_to(game_id, current_target, headers)
                    last_move_command_time = now
                    last_target_distance = None
                    last_progress_time = now

            # Shoot randomly when no enemies (less frequently)
            elif now - last_shoot_time >= 1.0:
                # Pick a random target position
                random_target = [
                    position[0] + random.uniform(-30, 30),
                    position[1] + random.uniform(-2, 2),
                    position[2] + random.uniform(-30, 30),
                ]
                await shoot(game_id, random_target, headers)
                last_shoot_time = now

            # Track progress toward target
            if current_target is not None:
                target_dist = distance(position, current_target)
                if last_target_distance is None or target_dist < last_target_distance - 0.1:
                    last_progress_time = now
                last_target_distance = target_dist

            # Check if stuck (not making progress toward target)
            if last_position is not None and current_target is not None:
                moved = distance(position, last_position)
                if moved < stuck_threshold * 0.1:  # Barely moved this tick
                    if stuck_start_time is None:
                        stuck_start_time = now
                    elif now - stuck_start_time > stuck_timeout:
                        print(f"\n[STUCK] No progress for {stuck_timeout}s, picking new target")
                        current_target = None
                        stuck_start_time = None
                        last_target_distance = None
                        last_progress_time = None
                else:
                    stuck_start_time = None  # Reset if making progress
            last_position = position[:]
            last_position_time = now

            # Check if we need a new target (no target, reached it, or stuck)
            need_new_target = (
                current_target is None or
                distance(position, current_target) < target_reached_threshold
            )

            if need_new_target and not nearest_enemy:
                # Pick a new random target
                current_target = random_position(center=[0, position[1], 0], radius=25.0)
                await move_to(game_id, current_target, headers)
                last_move_command_time = now
                stuck_start_time = None
                last_target_distance = None
                last_progress_time = now

            # Re-issue MoveTo while traveling to avoid stale targets
            if current_target is not None and now - last_move_command_time >= move_command_interval:
                await move_to(game_id, current_target, headers)
                last_move_command_time = now

            # If distance to target isn't shrinking for a while, pick a new target
            if (
                current_target is not None
                and last_progress_time is not None
                and now - last_progress_time > progress_timeout
            ):
                print(f"\n[STUCK] Target distance not shrinking for {progress_timeout}s, re-pathing")
                current_target = random_position(center=[0, position[1], 0], radius=25.0)
                await move_to(game_id, current_target, headers)
                last_move_command_time = now
                last_target_distance = None
                last_progress_time = now

            # 10 Hz loop
            await asyncio.sleep(0.1)

    except KeyboardInterrupt:
        print("\n\nStopping agent...")
    finally:
        async with state_lock:
            state["running"] = False
        logger_task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await logger_task
        await leave_game(game_id, headers)


def main():
    parser = argparse.ArgumentParser(description="Test agent for Clawblox arena game")
    parser.add_argument("--api-key", help="API key for authentication")
    parser.add_argument(
        "--register", action="store_true", help="Register a new agent first"
    )
    parser.add_argument(
        "--leave", action="store_true", help="Leave all games and exit"
    )
    parser.add_argument(
        "--duration", type=float, default=60.0, help="Run for N seconds (default: 60)"
    )
    args = parser.parse_args()

    api_key = args.api_key or os.getenv("CLAWBLOX_API_KEY")

    if args.register:
        api_key = asyncio.run(register_agent())
        print(f"\nSet this env var to reuse: export CLAWBLOX_API_KEY={api_key}\n")
    elif not api_key:
        print("Error: No API key provided.")
        print("Use --api-key YOUR_KEY or set CLAWBLOX_API_KEY env var")
        print("Or use --register to create a new agent")
        sys.exit(1)

    if args.leave:
        headers = {"Authorization": f"Bearer {api_key}"}
        asyncio.run(leave_all_games(headers))
        return

    asyncio.run(run_agent(api_key, duration=args.duration))


if __name__ == "__main__":
    main()
