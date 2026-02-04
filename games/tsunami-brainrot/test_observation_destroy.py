#!/usr/bin/env python3
"""
Test new observation attributes and Destroy command for Tsunami Brainrot game.

Tests:
1. Player attributes: PlacedBrainrots, CarriedBrainrots, BaseMaxBrainrots,
   NextSpeedCost, ZoneInfo
2. GameState folder (world entity) with real-time attributes: WaveTimeRemaining,
   WaveInterval, ActiveWaveCount, SpawnedBrainrots, ZoneInfo
3. World entity attributes (brainrot Value)
4. Destroy command to remove placed brainrots
"""

import json
import os
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
GAME_ID = "0a62727e-b45e-4175-be9f-1070244f8885"  # Tsunami Brainrot

# Game constants
BASE_ZONE_X_START = 350
COLLECTION_RANGE = 5


def distance_xz(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def get_api_key() -> str:
    """Get or register an API key"""
    key = os.getenv("CLAWBLOX_API_KEY")
    if key:
        return key

    # Register new agent
    import random
    name = f"test_obs_{random.randint(1000, 9999)}"
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": name, "description": "Observation test agent"},
        timeout=10,
    )
    if resp.status_code == 200:
        return resp.json()["agent"]["api_key"]
    raise RuntimeError(f"Failed to register: {resp.text}")


def test_observation_attributes():
    """Test that all new observation attributes are present"""
    print("\n=== Testing Observation Attributes ===")

    api_key = get_api_key()
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join game
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"FAIL: Could not join game: {resp.text}")
        return False
    print("Joined game")

    time.sleep(0.5)

    try:
        # Get observation
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        if resp.status_code != 200:
            print(f"FAIL: Observe failed: {resp.status_code}")
            return False

        obs = resp.json()
        attrs = obs["player"].get("attributes", {})

        print(f"\nPlayer attributes received:")
        for key, value in sorted(attrs.items()):
            display_value = value
            if isinstance(value, str) and len(value) > 60:
                display_value = value[:60] + "..."
            print(f"  {key}: {display_value}")

        # Check required attributes
        # Note: WaveTimeRemaining moved to GameState folder (world entity), ZoneInfo is both
        required_attrs = [
            "BaseMaxBrainrots",
            "NextSpeedCost",
            "ZoneInfo",
            "PlacedBrainrots",
            "CarriedBrainrots",
        ]

        missing = []
        for attr in required_attrs:
            if attr not in attrs:
                missing.append(attr)

        if missing:
            print(f"\nFAIL: Missing attributes: {missing}")
            return False

        # Validate ZoneInfo is valid JSON array
        zone_info = attrs.get("ZoneInfo")
        if zone_info:
            try:
                zones = json.loads(zone_info)
                print(f"\nZoneInfo parsed ({len(zones)} zones):")
                for z in zones:
                    print(f"  {z['name']}: xMin={z['xMin']}, xMax={z['xMax']}, value={z['value']}")
            except json.JSONDecodeError as e:
                print(f"\nFAIL: ZoneInfo is not valid JSON: {e}")
                return False

        # Validate PlacedBrainrots is valid JSON
        placed = attrs.get("PlacedBrainrots")
        if placed:
            try:
                placed_list = json.loads(placed)
                print(f"\nPlacedBrainrots: {len(placed_list)} items")
            except json.JSONDecodeError as e:
                print(f"\nFAIL: PlacedBrainrots is not valid JSON: {e}")
                return False

        # Validate CarriedBrainrots is valid JSON
        carried = attrs.get("CarriedBrainrots")
        if carried:
            try:
                carried_list = json.loads(carried)
                print(f"CarriedBrainrots: {len(carried_list)} items")
            except json.JSONDecodeError as e:
                print(f"\nFAIL: CarriedBrainrots is not valid JSON: {e}")
                return False

        print(f"\nBaseMaxBrainrots: {attrs.get('BaseMaxBrainrots')}")
        print(f"NextSpeedCost: {attrs.get('NextSpeedCost')}")

        # Check GameState folder for WaveTimeRemaining
        entities = obs.get("world", {}).get("entities", [])
        game_state = next((e for e in entities if e.get("name") == "GameState"), None)
        if game_state:
            gs_attrs = game_state.get("attributes", {})
            print(f"\nGameState folder found:")
            print(f"  WaveTimeRemaining: {gs_attrs.get('WaveTimeRemaining', 'N/A')}")
            print(f"  WaveInterval: {gs_attrs.get('WaveInterval', 'N/A')}")
            print(f"  ActiveWaveCount: {gs_attrs.get('ActiveWaveCount', 'N/A')}")
            print(f"  SpawnedBrainrots: {gs_attrs.get('SpawnedBrainrots', 'N/A')}")
            if "WaveTimeRemaining" not in gs_attrs:
                print("\nFAIL: GameState missing WaveTimeRemaining")
                return False
        else:
            print("\nFAIL: GameState folder not found in world entities")
            return False

        print("\nPASS: All observation attributes present and valid")
        return True

    finally:
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print("Left game")


def test_world_entity_attributes():
    """Test that world entities (brainrots) have attributes like Value"""
    print("\n=== Testing World Entity Attributes ===")

    api_key = get_api_key()
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join game
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"FAIL: Could not join game: {resp.text}")
        return False
    print("Joined game")

    time.sleep(0.5)

    try:
        # Get observation
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        if resp.status_code != 200:
            print(f"FAIL: Observe failed: {resp.status_code}")
            return False

        obs = resp.json()
        entities = obs.get("world", {}).get("entities", [])

        # Find brainrots
        brainrots = [e for e in entities if e.get("name") == "Brainrot"]
        print(f"\nFound {len(brainrots)} brainrots in world")

        if not brainrots:
            print("WARN: No brainrots found to check attributes")
            return True  # Not a failure, just no brainrots spawned yet

        # Check if brainrots have attributes
        brainrots_with_attrs = [b for b in brainrots if b.get("attributes")]
        print(f"Brainrots with attributes: {len(brainrots_with_attrs)}")

        if brainrots_with_attrs:
            sample = brainrots_with_attrs[0]
            print(f"\nSample brainrot attributes:")
            for key, value in sample.get("attributes", {}).items():
                print(f"  {key}: {value}")

            # Check for Value attribute
            if "Value" in sample.get("attributes", {}):
                print("\nPASS: Brainrot entities have Value attribute")
                return True
            else:
                print("\nFAIL: Brainrot missing Value attribute")
                return False
        else:
            print("\nFAIL: No brainrots have attributes field")
            return False

    finally:
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print("Left game")


def test_destroy_command():
    """Test the Destroy command to remove placed brainrots"""
    print("\n=== Testing Destroy Command ===")

    api_key = get_api_key()
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join game
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"FAIL: Could not join game: {resp.text}")
        return False
    print("Joined game")

    time.sleep(0.5)

    try:
        # Step 1: Get initial observation and find a brainrot
        # Target brainrots close to safe zone to minimize wave risk
        print("\nStep 1: Finding and collecting a brainrot (targeting nearby ones)...")

        max_attempts = 150  # More time to reach brainrots
        collected = False

        for attempt in range(max_attempts):
            resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
            if resp.status_code != 200:
                continue

            obs = resp.json()
            pos = obs["player"]["position"]
            attrs = obs["player"].get("attributes", {})
            entities = obs.get("world", {}).get("entities", [])

            carried_count = attrs.get("CarriedCount", 0)

            if carried_count > 0:
                print(f"  Already carrying {carried_count} brainrot(s)")
                collected = True
                break

            # Find nearest collectible brainrot
            brainrots = [e for e in entities if e.get("name") == "Brainrot" and e["position"][0] < BASE_ZONE_X_START]

            if not brainrots:
                print(f"  Attempt {attempt + 1}: No brainrots visible, waiting...")
                time.sleep(0.5)
                continue

            # Find nearest
            nearest = min(brainrots, key=lambda b: distance_xz(pos, b["position"]))
            dist = distance_xz(pos, nearest["position"])

            if dist < COLLECTION_RANGE:
                # Collect it
                resp = requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "Collect"},
                    timeout=5
                )
                print(f"  Attempt {attempt + 1}: Collected brainrot!")
                collected = True
                break
            else:
                # Move toward it
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "MoveTo", "data": {"position": nearest["position"]}},
                    timeout=5
                )
                if attempt % 10 == 0:
                    print(f"  Attempt {attempt + 1}: Moving to brainrot, dist={dist:.1f}")

            time.sleep(0.2)

        if not collected:
            print("FAIL: Could not collect a brainrot")
            return False

        # Step 2: Return to base and deposit
        print("\nStep 2: Returning to base and depositing...")

        for attempt in range(max_attempts):
            resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
            if resp.status_code != 200:
                continue

            obs = resp.json()
            pos = obs["player"]["position"]
            attrs = obs["player"].get("attributes", {})

            base_center_x = attrs.get("BaseCenterX", 375)
            base_center_z = attrs.get("BaseCenterZ", 0)

            dx = abs(pos[0] - base_center_x)
            dz = abs(pos[2] - base_center_z)

            if dx < 15 and dz < 10:
                # At base, deposit
                resp = requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "Deposit"},
                    timeout=5
                )
                print(f"  Deposited brainrot!")
                break
            else:
                # Move to base
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "MoveTo", "data": {"position": [base_center_x, pos[1], base_center_z]}},
                    timeout=5
                )
                if attempt % 10 == 0:
                    print(f"  Attempt {attempt + 1}: Moving to base, dx={dx:.1f}, dz={dz:.1f}")

            time.sleep(0.2)

        # Step 3: Verify we have placed brainrots
        print("\nStep 3: Verifying placed brainrots...")
        time.sleep(1.0)  # Wait for server to update

        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        obs = resp.json()
        attrs = obs["player"].get("attributes", {})

        placed_json = attrs.get("PlacedBrainrots", "[]")
        placed_list = json.loads(placed_json)

        print(f"  Placed brainrots: {len(placed_list)}")
        for p in placed_list:
            print(f"    index={p.get('index')}, value={p.get('value')}, zone={p.get('zone')}")

        if not placed_list:
            print("FAIL: No placed brainrots to destroy")
            return False

        initial_count = len(placed_list)

        # Step 4: Destroy the first brainrot
        print(f"\nStep 4: Destroying brainrot at index 1...")

        resp = requests.post(
            f"{API_BASE}/games/{GAME_ID}/input",
            headers=headers,
            json={"type": "Destroy", "data": {"index": 1}},
            timeout=5
        )

        if resp.status_code != 200:
            print(f"FAIL: Destroy request failed: {resp.status_code} {resp.text}")
            return False

        print(f"  Destroy command sent, status={resp.status_code}")

        # Step 5: Verify brainrot was destroyed
        print("\nStep 5: Verifying destruction...")
        time.sleep(0.3)

        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        obs = resp.json()
        attrs = obs["player"].get("attributes", {})

        placed_json = attrs.get("PlacedBrainrots", "[]")
        placed_list = json.loads(placed_json)

        final_count = len(placed_list)
        print(f"  Placed brainrots after destroy: {final_count}")

        if final_count < initial_count:
            print(f"\nPASS: Brainrot destroyed ({initial_count} -> {final_count})")
            return True
        else:
            print(f"\nFAIL: Brainrot count unchanged ({initial_count} -> {final_count})")
            return False

    finally:
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print("Left game")


def main():
    print(f"API: {API_BASE}")
    print(f"Game: {GAME_ID}")
    print("=" * 60)

    results = []

    # Test 1: Observation attributes
    results.append(("Observation Attributes", test_observation_attributes()))

    # Test 2: World entity attributes
    results.append(("World Entity Attributes", test_world_entity_attributes()))

    # Test 3: Destroy command
    results.append(("Destroy Command", test_destroy_command()))

    # Summary
    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)

    all_passed = True
    for name, passed in results:
        status = "PASS" if passed else "FAIL"
        print(f"  {name}: {status}")
        if not passed:
            all_passed = False

    print()
    if all_passed:
        print("All tests passed!")
        sys.exit(0)
    else:
        print("Some tests failed!")
        sys.exit(1)


if __name__ == "__main__":
    main()
