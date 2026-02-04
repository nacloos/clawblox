#!/usr/bin/env python3
"""
Test AFK detection and player kick functionality.

Test 1: AFK player gets kicked after 5 minutes of inactivity
Test 2: Active player is NOT kicked (activity resets AFK timer)
"""

import argparse
import json
import os
from pathlib import Path
import random
import sys
import time

import requests

# Load .env file if present
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
GAME_ID = "0a62727e-b45e-4175-be9f-1070244f8885"  # Tsunami Brainrot
KEYS_CACHE = Path("/tmp/clawblox_afk_test_keys.json")

# AFK timeout is 5 minutes (300 seconds)
AFK_TIMEOUT_SECONDS = 300
# Check interval during AFK test
CHECK_INTERVAL_SECONDS = 30
# Activity interval for active player test (send input every N seconds)
ACTIVITY_INTERVAL_SECONDS = 60


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
            json={"name": name, "description": "AFK test agent"},
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
        name = f"afk_test_{random.randint(1000, 9999)}"
        print(f"Registering {name}...")
        key = register_agent(name)
        if key:
            keys.append(key)
    save_cached_keys(keys)
    return keys[:num]


def leave_all_games(headers: dict):
    """Leave all games the agent is currently in"""
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        if resp.status_code == 200:
            for g in resp.json().get("games", []):
                requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass


def join_game(headers: dict) -> bool:
    """Join the tsunami game, returns True if successful"""
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=10)
    return resp.status_code == 200


def can_observe(headers: dict) -> bool:
    """Check if player can observe (i.e., is still in the game)"""
    try:
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=10)
        return resp.status_code == 200
    except:
        return False


def get_observation(headers: dict) -> dict | None:
    """Get observation data, returns None if failed"""
    try:
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=10)
        if resp.status_code == 200:
            return resp.json()
    except:
        pass
    return None


def send_move_input(headers: dict, position: list) -> bool:
    """Send a MoveTo input to keep player active"""
    try:
        resp = requests.post(
            f"{API_BASE}/games/{GAME_ID}/input",
            headers=headers,
            json={"type": "MoveTo", "data": {"position": position}},
            timeout=5,
        )
        return resp.status_code == 200
    except:
        return False


def test_afk_player_kicked(api_key: str) -> bool:
    """
    Test that an AFK player gets kicked after 5 minutes of inactivity.

    Returns True if test passes (player was kicked), False otherwise.
    """
    print("\n" + "=" * 60)
    print("TEST: AFK Player Gets Kicked")
    print("=" * 60)
    print(f"AFK timeout: {AFK_TIMEOUT_SECONDS} seconds ({AFK_TIMEOUT_SECONDS // 60} minutes)")
    print()

    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    leave_all_games(headers)
    time.sleep(0.5)

    # Join the game
    print("Joining game...")
    if not join_game(headers):
        print("FAIL: Could not join game")
        return False
    print("Joined successfully!")

    # Verify we can observe
    obs = get_observation(headers)
    if not obs:
        print("FAIL: Could not get initial observation")
        return False

    pos = obs["player"]["position"]
    print(f"Initial position: ({pos[0]:.1f}, {pos[1]:.1f}, {pos[2]:.1f})")
    print()

    # Now go AFK - do NOT send any inputs
    print("Going AFK (no inputs will be sent)...")
    print(f"Will check status every {CHECK_INTERVAL_SECONDS} seconds")
    print()

    start_time = time.time()
    max_wait_time = AFK_TIMEOUT_SECONDS + 60  # Wait up to 1 minute past timeout

    while True:
        elapsed = time.time() - start_time
        remaining = AFK_TIMEOUT_SECONDS - elapsed

        if elapsed > max_wait_time:
            print(f"\nFAIL: Player was NOT kicked after {elapsed:.0f} seconds (expected kick at {AFK_TIMEOUT_SECONDS}s)")
            leave_all_games(headers)
            return False

        # Check if we can still observe
        if can_observe(headers):
            if remaining > 0:
                print(f"  [{elapsed:5.0f}s] Still in game... (kick expected in {remaining:.0f}s)")
            else:
                print(f"  [{elapsed:5.0f}s] Still in game... (should have been kicked by now)")
        else:
            print(f"\n  [{elapsed:5.0f}s] Player was KICKED!")
            if elapsed >= AFK_TIMEOUT_SECONDS - 10:  # Allow 10s tolerance
                print(f"\nPASS: Player kicked after {elapsed:.0f} seconds of inactivity")
                return True
            else:
                print(f"\nFAIL: Player kicked too early ({elapsed:.0f}s < {AFK_TIMEOUT_SECONDS}s)")
                return False

        time.sleep(CHECK_INTERVAL_SECONDS)


def test_activity_prevents_afk_kick(api_key: str) -> bool:
    """
    Test that an active player is NOT kicked.
    Sends periodic MoveTo inputs to keep the player active.

    Returns True if test passes (player was NOT kicked), False otherwise.
    """
    print("\n" + "=" * 60)
    print("TEST: Activity Prevents AFK Kick")
    print("=" * 60)
    print(f"Will send activity every {ACTIVITY_INTERVAL_SECONDS} seconds")
    print(f"Test duration: {AFK_TIMEOUT_SECONDS + 60} seconds (past AFK timeout)")
    print()

    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    leave_all_games(headers)
    time.sleep(0.5)

    # Join the game
    print("Joining game...")
    if not join_game(headers):
        print("FAIL: Could not join game")
        return False
    print("Joined successfully!")

    # Verify we can observe
    obs = get_observation(headers)
    if not obs:
        print("FAIL: Could not get initial observation")
        return False

    pos = obs["player"]["position"]
    print(f"Initial position: ({pos[0]:.1f}, {pos[1]:.1f}, {pos[2]:.1f})")
    print()

    # Stay active by sending periodic inputs
    print("Staying active with periodic MoveTo inputs...")
    print()

    start_time = time.time()
    last_activity_time = start_time
    test_duration = AFK_TIMEOUT_SECONDS + 60  # Run 1 minute past AFK timeout

    while True:
        elapsed = time.time() - start_time

        if elapsed > test_duration:
            break

        # Send activity if interval has passed
        time_since_activity = time.time() - last_activity_time
        if time_since_activity >= ACTIVITY_INTERVAL_SECONDS:
            # Get current position and send a small move
            obs = get_observation(headers)
            if obs:
                pos = obs["player"]["position"]
                # Move slightly in a random direction
                new_pos = [
                    pos[0] + random.uniform(-5, 5),
                    pos[1],
                    pos[2] + random.uniform(-5, 5),
                ]
                if send_move_input(headers, new_pos):
                    print(f"  [{elapsed:5.0f}s] Sent MoveTo input (staying active)")
                    last_activity_time = time.time()
                else:
                    print(f"  [{elapsed:5.0f}s] WARNING: MoveTo input failed")
            else:
                print(f"  [{elapsed:5.0f}s] WARNING: Could not get observation")

        # Check if still in game
        if not can_observe(headers):
            print(f"\nFAIL: Player was kicked at {elapsed:.0f}s despite being active!")
            return False

        time.sleep(CHECK_INTERVAL_SECONDS)

    # Final check
    if can_observe(headers):
        print(f"\nPASS: Player was NOT kicked after {test_duration:.0f} seconds (stayed active)")
        leave_all_games(headers)
        return True
    else:
        print(f"\nFAIL: Player was kicked at some point")
        return False


def main():
    parser = argparse.ArgumentParser(description="Test AFK detection and kick functionality")
    parser.add_argument("--test", choices=["afk", "active", "both"], default="both",
                        help="Which test to run: 'afk' (AFK kick), 'active' (activity prevents kick), or 'both'")
    parser.add_argument("--api-key", type=str, help="API key (or uses env var)")
    args = parser.parse_args()

    print(f"API: {API_BASE}")
    print(f"Game: {GAME_ID}")

    # Get API keys (need 2 for running both tests, 1 otherwise)
    num_keys = 2 if args.test == "both" else 1
    if args.api_key:
        api_keys = [args.api_key]
        if num_keys > 1:
            api_keys.extend(get_api_keys(num_keys - 1))
    else:
        api_keys = get_api_keys(num_keys)

    print(f"Got {len(api_keys)} API key(s)")

    results = {}

    if args.test in ("afk", "both"):
        results["afk_kick"] = test_afk_player_kicked(api_keys[0])

    if args.test in ("active", "both"):
        key_idx = 1 if args.test == "both" and len(api_keys) > 1 else 0
        results["activity_prevents_kick"] = test_activity_prevents_afk_kick(api_keys[key_idx])

    # Summary
    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)
    all_passed = True
    for test_name, passed in results.items():
        status = "PASS" if passed else "FAIL"
        print(f"  {test_name}: {status}")
        if not passed:
            all_passed = False

    print()
    if all_passed:
        print("All tests PASSED!")
        sys.exit(0)
    else:
        print("Some tests FAILED!")
        sys.exit(1)


if __name__ == "__main__":
    main()
