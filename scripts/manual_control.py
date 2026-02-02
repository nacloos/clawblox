#!/usr/bin/env python3
"""
Manual control for testing player movement.

Arrow keys to move, Q to quit.

Usage:
    uv run scripts/manual_control.py
"""

import os
import sys
import termios
import tty
from pathlib import Path

from dotenv import load_dotenv
import requests

load_dotenv(Path(__file__).parent / ".env")

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
API_KEY = os.getenv("CLAWBLOX_API_KEY")

MOVE_DISTANCE = 5.0  # How far to move per keypress


def get_key():
    """Read a single keypress."""
    fd = sys.stdin.fileno()
    old_settings = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        ch = sys.stdin.read(1)
        if ch == '\x1b':  # Escape sequence (arrow keys)
            ch += sys.stdin.read(2)
        return ch
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)


def observe(game_id: str, headers: dict) -> dict:
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers)
    resp.raise_for_status()
    return resp.json()


def move_to(game_id: str, position: list, headers: dict):
    resp = requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": position}},
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

    print("\nControls:")
    print("  Arrow keys: Move")
    print("  WASD: Move")
    print("  Q: Quit")
    print()

    try:
        while True:
            # Get current position
            obs = observe(game_id, headers)
            pos = obs["player"]["position"]
            print(f"\rPos: ({pos[0]:6.1f}, {pos[1]:5.1f}, {pos[2]:6.1f})  ", end="", flush=True)

            key = get_key()

            dx, dz = 0, 0

            # Arrow keys
            if key == '\x1b[A' or key == 'w':  # Up
                dz = -MOVE_DISTANCE
            elif key == '\x1b[B' or key == 's':  # Down
                dz = MOVE_DISTANCE
            elif key == '\x1b[C' or key == 'd':  # Right
                dx = MOVE_DISTANCE
            elif key == '\x1b[D' or key == 'a':  # Left
                dx = -MOVE_DISTANCE
            elif key == 'q' or key == '\x03':  # Q or Ctrl+C
                print("\nQuitting...")
                break
            else:
                continue

            # Calculate target position
            target = [pos[0] + dx, pos[1], pos[2] + dz]
            move_to(game_id, target, headers)

    except KeyboardInterrupt:
        print("\nQuitting...")
    finally:
        leave_game(game_id, headers)
        print("Left game")


if __name__ == "__main__":
    main()
