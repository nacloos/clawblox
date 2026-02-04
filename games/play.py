#!/usr/bin/env python3
"""
Manual keyboard controller for Clawblox games.
Use WASD keys to move the player around.
"""

import argparse
import json
import os
import random
import sys
import threading
import time
from pathlib import Path

import readchar
import requests

# Load .env file if present (project root is ../)
env_path = Path(__file__).parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
KEYS_CACHE = Path("/tmp/clawblox_keys.json")

# Available games
GAMES = {
    "tsunami": "0a62727e-b45e-4175-be9f-1070244f8885",
    "flat": "26c869ee-da7b-48a4-a198-3daa870ef652",
}

# Movement settings
MOVE_SPEED = 20.0  # Distance per movement command

# Shared state
movement = {"x": 0, "z": 0}
running = True
last_key = ""
stop_requested = False


def load_cached_keys() -> list[str]:
    if KEYS_CACHE.exists():
        try:
            return json.loads(KEYS_CACHE.read_text()).get("keys", [])
        except:
            pass
    return []


def save_cached_keys(keys: list[str]):
    KEYS_CACHE.write_text(json.dumps({"keys": keys}))


def register_agent(api_base: str, name: str) -> str | None:
    try:
        resp = requests.post(
            f"{api_base}/agents/register",
            json={"name": name, "description": "Manual keyboard player"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def get_api_key(api_base: str) -> str:
    """Get or register an API key"""
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        return env_key

    keys = load_cached_keys()
    if keys:
        return keys[0]

    name = f"manual_player_{random.randint(1000, 9999)}"
    print(f"Registering {name}...")
    key = register_agent(api_base, name)
    if key:
        save_cached_keys([key])
        return key

    print("Failed to get API key")
    sys.exit(1)


def keyboard_thread():
    """Thread that reads keyboard input"""
    global movement, running, last_key, stop_requested

    while running:
        try:
            key = readchar.readkey()
            last_key = key
            key_lower = key.lower() if len(key) == 1 else ""

            if key_lower == 'w':
                movement = {"x": 0, "z": -1}
            elif key_lower == 's':
                movement = {"x": 0, "z": 1}
            elif key_lower == 'a':
                movement = {"x": -1, "z": 0}
            elif key_lower == 'd':
                movement = {"x": 1, "z": 0}
            elif key_lower == 'q' or key == readchar.key.ESCAPE:
                running = False
            elif key_lower in ('x', 'e') or key == ' ':
                # X, E, or SPACE = stop
                print(f"\n[STOP KEY] Pressed: '{repr(key)}'")
                movement = {"x": 0, "z": 0}
                stop_requested = True
        except Exception as e:
            if running:  # Only log if not shutting down
                print(f"\n[KB ERROR] {e}")


def main():
    global running, movement, stop_requested

    parser = argparse.ArgumentParser(description="Manual keyboard controller for Clawblox games")
    parser.add_argument(
        "game",
        nargs="?",
        default="flat",
        choices=list(GAMES.keys()),
        help="Game to play (default: flat)"
    )
    args = parser.parse_args()

    game_id = GAMES[args.game]

    print("=" * 60)
    print(f"Clawblox - Manual Keyboard Controller ({args.game})")
    print("=" * 60)
    print("Controls:")
    print("  W - Move forward (negative Z)")
    print("  S - Move backward (positive Z)")
    print("  A - Move left (negative X)")
    print("  D - Move right (positive X)")
    print("  E, X, or SPACE - Stop moving")
    print("  Q or ESC - Quit")
    print("=" * 60)

    api_key = get_api_key(API_BASE)
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games first
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join the game
    print(f"Joining game {game_id}...")
    resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Failed to join: {resp.text}")
        sys.exit(1)
    print(f"Join response: {resp.json()}")
    print("Joined! Press any movement key to start.\n")

    time.sleep(0.5)

    # Start keyboard thread
    kb_thread = threading.Thread(target=keyboard_thread, daemon=True)
    kb_thread.start()

    last_pos = None

    try:
        while running:
            # Observe current state
            try:
                resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers, timeout=5)
                if resp.status_code != 200:
                    print(f"\nObserve failed: {resp.status_code} {resp.text}")
                    time.sleep(0.5)
                    continue

                obs = resp.json()
                pos = obs["player"]["position"]
                attrs = obs["player"].get("attributes", {})

                # Display status
                money = attrs.get("Money", 0)
                speed_level = attrs.get("SpeedLevel", 1)
                carried = attrs.get("CarriedCount", 0)

                # Calculate movement delta
                moved = ""
                if last_pos:
                    dx = pos[0] - last_pos[0]
                    dz = pos[2] - last_pos[2]
                    if abs(dx) > 0.1 or abs(dz) > 0.1:
                        moved = f" delta=({dx:+.1f}, {dz:+.1f})"

                # Print status on single line
                dir_str = ""
                if movement["x"] != 0 or movement["z"] != 0:
                    dir_str = f" -> ({movement['x']:+d}, {movement['z']:+d})"
                status = f"\rPos: ({pos[0]:7.1f}, {pos[2]:7.1f}) | $:{money:7.0f} | Spd:{speed_level} | Carry:{carried}{dir_str}{moved}    "
                print(status, end="", flush=True)

                # Send movement command if moving
                if movement["x"] != 0 or movement["z"] != 0:
                    target = [
                        pos[0] + movement["x"] * MOVE_SPEED,
                        pos[1],
                        pos[2] + movement["z"] * MOVE_SPEED
                    ]
                    requests.post(
                        f"{API_BASE}/games/{game_id}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": target}},
                        timeout=5
                    )
                if stop_requested:
                    # Stop by sending Stop command
                    stop_requested = False
                    print(f"\n[STOP] Sending Stop command...")
                    try:
                        stop_resp = requests.post(
                            f"{API_BASE}/games/{game_id}/input",
                            headers=headers,
                            json={"type": "Stop", "data": {}},
                            timeout=5
                        )
                        print(f"[STOP] Response: {stop_resp.status_code}")
                    except Exception as e:
                        print(f"[STOP] Error: {e}")

                last_pos = pos

            except requests.exceptions.RequestException as e:
                print(f"\nNetwork error: {e}")
                time.sleep(0.5)

            time.sleep(0.1)  # 10Hz loop

    except KeyboardInterrupt:
        print("\n\nStopping...")
    finally:
        running = False
        requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers, timeout=5)
        print("\nLeft game.")


if __name__ == "__main__":
    main()
