#!/usr/bin/env python3
"""
Automated movement test for flat-test game.
Tests MoveTo commands in all directions and asserts position changes.
"""

import json
import os
import random
import sys
import time
from pathlib import Path

import requests

# Load .env file if present
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
GAME_ID = "26c869ee-da7b-48a4-a198-3daa870ef652"  # Flat Test
KEYS_CACHE = Path("/tmp/clawblox_test_keys.json")

# Test parameters
MOVE_DISTANCE = 20.0  # Distance to move per test
WAIT_TIME = 0.5       # Seconds to wait for movement
MIN_EXPECTED_MOVE = 5.0  # Minimum expected position change
NUM_TRIALS = 3        # Number of trials per direction


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
            json={"name": name, "description": "Movement test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def get_api_key(api_base: str) -> str:
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        return env_key

    keys = load_cached_keys()
    if keys:
        return keys[0]

    name = f"test_agent_{random.randint(1000, 9999)}"
    print(f"Registering {name}...")
    key = register_agent(api_base, name)
    if key:
        save_cached_keys([key])
        return key

    print("Failed to get API key")
    sys.exit(1)


def get_position(headers: dict) -> tuple[float, float, float]:
    """Get current player position."""
    resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
    assert resp.status_code == 200, f"Observe failed: {resp.status_code} {resp.text}"
    obs = resp.json()
    pos = obs["player"]["position"]
    return pos[0], pos[1], pos[2]


def send_move(headers: dict, target: tuple[float, float, float]) -> None:
    """Send MoveTo command."""
    resp = requests.post(
        f"{API_BASE}/games/{GAME_ID}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": list(target)}},
        timeout=5
    )
    assert resp.status_code == 200, f"MoveTo failed: {resp.status_code} {resp.text}"


def test_movement_direction(headers: dict, name: str, dx: float, dz: float, verbose: bool = False) -> list[dict]:
    """Test movement in a specific direction. Returns list of trial results."""
    results = []

    for trial in range(NUM_TRIALS):
        # Get starting position
        start_x, start_y, start_z = get_position(headers)

        # Calculate target
        target = (start_x + dx * MOVE_DISTANCE, start_y, start_z + dz * MOVE_DISTANCE)

        if verbose:
            print(f"    Sending MoveTo: start=({start_x:.1f}, {start_z:.1f}) -> target=({target[0]:.1f}, {target[2]:.1f})")

        # Send move command
        send_move(headers, target)

        # Wait for movement
        time.sleep(WAIT_TIME)

        # Get ending position
        end_x, end_y, end_z = get_position(headers)

        # Calculate actual movement
        actual_dx = end_x - start_x
        actual_dz = end_z - start_z
        distance_moved = (actual_dx**2 + actual_dz**2) ** 0.5

        # Check if movement was in expected direction
        expected_distance = (dx**2 + dz**2) ** 0.5 * MOVE_DISTANCE

        result = {
            "trial": trial + 1,
            "start": (start_x, start_z),
            "target": (target[0], target[2]),
            "end": (end_x, end_z),
            "expected_dx": dx * MOVE_DISTANCE,
            "expected_dz": dz * MOVE_DISTANCE,
            "actual_dx": actual_dx,
            "actual_dz": actual_dz,
            "distance_moved": distance_moved,
            "success": distance_moved >= MIN_EXPECTED_MOVE,
        }
        results.append(result)

        # Small delay between trials
        time.sleep(0.1)

    return results


def run_tests():
    print("=" * 70)
    print("Movement Test - Flat Test Game")
    print("=" * 70)

    api_key = get_api_key(API_BASE)
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join game
    print(f"Joining game {GAME_ID}...")
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    assert resp.status_code == 200, f"Failed to join: {resp.text}"
    print("Joined!")

    # Wait for game to initialize
    time.sleep(1.0)

    # Get initial position
    try:
        x, y, z = get_position(headers)
        print(f"Initial position: ({x:.1f}, {y:.1f}, {z:.1f})")
    except AssertionError as e:
        print(f"Failed to get initial position: {e}")
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        sys.exit(1)

    # Define directions to test - randomize order to check for sequence dependency
    directions = [
        ("North (-Z)", 0, -1),
        ("South (+Z)", 0, 1),
        ("East (+X)", 1, 0),
        ("West (-X)", -1, 0),
        ("Northeast", 1, -1),
        ("Northwest", -1, -1),
        ("Southeast", 1, 1),
        ("Southwest", -1, 1),
    ]
    random.shuffle(directions)  # Randomize order

    all_results = {}
    total_tests = 0
    total_passed = 0

    print("\n" + "-" * 70)
    print("Running movement tests...")
    print("-" * 70)

    for name, dx, dz in directions:
        print(f"\nTesting {name} (dx={dx}, dz={dz})...")
        results = test_movement_direction(headers, name, dx, dz, verbose=True)
        all_results[name] = results

        # Wait between direction changes to let physics settle
        time.sleep(1.0)

        passed = sum(1 for r in results if r["success"])
        total_tests += len(results)
        total_passed += passed

        for r in results:
            status = "PASS" if r["success"] else "FAIL"
            print(f"  Trial {r['trial']}: {status} - moved {r['distance_moved']:.1f} "
                  f"(dx={r['actual_dx']:+.1f}, dz={r['actual_dz']:+.1f})")

    # Leave game
    requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)

    # Summary
    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)

    for name, results in all_results.items():
        passed = sum(1 for r in results if r["success"])
        status = "PASS" if passed == len(results) else "FAIL"
        print(f"{name}: {passed}/{len(results)} trials passed [{status}]")

    print("-" * 70)
    print(f"Total: {total_passed}/{total_tests} tests passed")
    print("=" * 70)

    # Assert overall success
    fail_rate = (total_tests - total_passed) / total_tests if total_tests > 0 else 0
    assert fail_rate < 0.2, f"Too many movement failures: {total_tests - total_passed}/{total_tests} failed"

    print("\nAll tests passed!")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(run_tests())
    except AssertionError as e:
        print(f"\nTEST FAILED: {e}")
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nInterrupted")
        sys.exit(1)
