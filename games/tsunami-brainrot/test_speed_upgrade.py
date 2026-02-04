#!/usr/bin/env python3
"""Test that speed upgrades actually affect player movement speed."""

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
GAME_ID = "0a62727e-b45e-4175-be9f-1070244f8885"  # Tsunami Brainrot


def distance_xz(a: list, b: list) -> float:
    """Calculate horizontal distance between two positions."""
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def measure_movement_speed(headers: dict, duration: float = 2.0) -> tuple[float, int]:
    """
    Move the player and measure how far they travel.
    Returns (distance_moved, speed_level).
    """
    # Get current position and speed level
    resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
    if resp.status_code != 200:
        raise RuntimeError(f"Observe failed: {resp.text}")

    obs = resp.json()
    start_pos = obs["player"]["position"]
    speed_level = obs["player"].get("attributes", {}).get("SpeedLevel", 1)

    # Move far in +X direction
    target = [start_pos[0] + 100, start_pos[1], start_pos[2]]
    resp = requests.post(
        f"{API_BASE}/games/{GAME_ID}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": target}},
        timeout=5
    )
    if resp.status_code != 200:
        raise RuntimeError(f"MoveTo failed: {resp.text}")

    # Wait for movement
    time.sleep(duration)

    # Get final position
    resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
    if resp.status_code != 200:
        raise RuntimeError(f"Observe failed: {resp.text}")

    end_pos = resp.json()["player"]["position"]
    distance = distance_xz(start_pos, end_pos)

    return distance, speed_level


def main():
    print(f"API: {API_BASE}")
    print(f"Game: {GAME_ID}")
    print("-" * 60)

    # Register agent
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": f"speed_test_{random.randint(1000, 9999)}", "description": "Speed upgrade test"},
        timeout=10
    )
    if resp.status_code != 200:
        print(f"Registration failed: {resp.text}")
        sys.exit(1)

    api_key = resp.json()["agent"]["api_key"]
    headers = {"Authorization": f"Bearer {api_key}"}
    print(f"Registered with key: {api_key[:20]}...")

    # Join game
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Join failed: {resp.text}")
        sys.exit(1)
    print("Joined game!")
    time.sleep(1)

    try:
        # Wait for waves to pass (stay in safe zone)
        print("\n=== Waiting for waves to pass (20s in safe zone) ===")
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        pos = resp.json()["player"]["position"]
        # Move to safe zone (base area, high X)
        requests.post(
            f"{API_BASE}/games/{GAME_ID}/input",
            headers=headers,
            json={"type": "MoveTo", "data": {"position": [380, pos[1], 0]}},
            timeout=5
        )
        time.sleep(20)
        print("Waves should have passed, starting test...")

        # Measure initial speed (level 1)
        print("\n=== Measuring speed at level 1 ===")
        distance1, level1 = measure_movement_speed(headers, duration=2.0)
        speed1 = distance1 / 2.0  # studs per second
        print(f"Speed level: {level1}")
        print(f"Distance moved in 2s: {distance1:.2f} studs")
        print(f"Effective speed: {speed1:.2f} studs/sec")

        # Earn money by collecting and depositing brainrots
        print("\n=== Earning money (collect + deposit brainrots) ===")
        money = 0
        attempts = 0
        max_attempts = 60  # Need ~100 coins for first upgrade

        while money < 100 and attempts < max_attempts:
            attempts += 1

            # Observe to find brainrots
            resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
            if resp.status_code != 200:
                continue

            obs = resp.json()
            pos = obs["player"]["position"]
            attrs = obs["player"].get("attributes", {})
            money = attrs.get("Money", 0)
            carried = attrs.get("CarriedCount", 0)
            entities = obs.get("world", {}).get("entities", [])

            # Find nearest brainrot in collection zone (X < 350)
            brainrots = [e for e in entities if e.get("name") == "Brainrot" and e["position"][0] < 350]

            if carried > 0:
                # Go deposit at base
                base_x = attrs.get("BaseCenterX", 375)
                base_z = attrs.get("BaseCenterZ", 0)
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "MoveTo", "data": {"position": [base_x, pos[1], base_z]}},
                    timeout=5
                )
                time.sleep(1.5)
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "Deposit"},
                    timeout=5
                )
                print(f"  Deposited! Money: {money}")
                time.sleep(0.5)
            elif brainrots:
                # Move to nearest brainrot and collect
                nearest = min(brainrots, key=lambda b: distance_xz(pos, b["position"]))
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "MoveTo", "data": {"position": nearest["position"]}},
                    timeout=5
                )
                time.sleep(1.5)
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "Collect"},
                    timeout=5
                )
                print(f"  Collected brainrot")
                time.sleep(0.5)
            else:
                # Roam to find brainrots
                target_x = random.uniform(-200, 200)
                requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "MoveTo", "data": {"position": [target_x, pos[1], 0]}},
                    timeout=5
                )
                time.sleep(1.0)

        # Check final money and save observation
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        obs = resp.json()
        money = obs["player"].get("attributes", {}).get("Money", 0)
        print(f"Money earned: {money}")

        # Save observation to file for inspection
        obs_path = Path(__file__).parent / "observation_sample.json"
        with open(obs_path, "w") as f:
            json.dump(obs, f, indent=2)
        print(f"Saved observation to {obs_path}")

        if money < 100:
            print("WARNING: Could not earn enough money to buy upgrade")
            print("Test inconclusive")
            return

        # Buy speed upgrade
        print("\n=== Buying speed upgrade ===")
        resp = requests.post(
            f"{API_BASE}/games/{GAME_ID}/input",
            headers=headers,
            json={"type": "BuySpeed"},
            timeout=5
        )
        if resp.status_code == 200:
            print("Bought speed upgrade!")
        else:
            print(f"BuySpeed response: {resp.status_code} - {resp.text}")

        # Small delay to process upgrade
        time.sleep(0.5)

        # Measure speed after upgrade
        print("\n=== Measuring speed after upgrade ===")
        distance2, level2 = measure_movement_speed(headers, duration=2.0)
        speed2 = distance2 / 2.0  # studs per second
        print(f"Speed level: {level2}")
        print(f"Distance moved in 2s: {distance2:.2f} studs")
        print(f"Effective speed: {speed2:.2f} studs/sec")

        # Compare results
        print("\n" + "=" * 60)
        print("RESULTS")
        print("=" * 60)
        print(f"Level 1 speed: {speed1:.2f} studs/sec")
        print(f"Level {level2} speed: {speed2:.2f} studs/sec")

        if level2 > level1:
            speed_increase = (speed2 - speed1) / speed1 * 100
            print(f"Speed increase: {speed_increase:.1f}%")

            if speed2 > speed1 * 1.1:  # At least 10% faster
                print("\nPASS: Speed upgrade is working correctly!")
            else:
                print("\nFAIL: Speed upgrade did not increase movement speed significantly")
                sys.exit(1)
        else:
            print("\nWARNING: Could not buy speed upgrade (maybe not enough money)")
            print("Test inconclusive - need money to test speed upgrades")

    finally:
        # Leave game
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print("\nLeft game.")


if __name__ == "__main__":
    main()
