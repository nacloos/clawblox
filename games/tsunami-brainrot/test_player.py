#!/usr/bin/env python3
"""
Test agent for Escape Tsunami For Brainrots.
Collects brainrots, deposits them, and buys speed upgrades.
"""

import argparse
import json
import os
from pathlib import Path
import random
import sys
import time

import requests

# Load .env file if present (project root is ../../)
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
GAME_ID = "a0000000-0000-0000-0000-000000000006"  # Tsunami Brainrot

# Map constants (rotated: X is long axis, Z is short axis)
# Safe zone: X >= 50, Collection zone: X < 50
SAFE_ZONE_X_START = 50  # X >= 50 is safe zone
COLLECTION_X_MIN = -100
COLLECTION_X_MAX = 50
MAP_HALF_WIDTH = 40  # Z from -40 to +40
COLLECTION_RANGE = 5

# Speed upgrade costs
SPEED_COSTS = [0, 100, 300, 700, 1500, 3000, 6000, 12000, 25000, 50000]


def distance_xz(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def find_nearest_brainrot(pos: list, entities: list) -> dict | None:
    """Find the nearest brainrot from world entities"""
    nearest = None
    nearest_dist = float('inf')

    for entity in entities:
        if entity.get("name") == "Brainrot":
            # Ignore placed brainrots in the safe zone (base area)
            if entity["position"][0] >= SAFE_ZONE_X_START:
                continue
            dist = distance_xz(pos, entity["position"])
            if dist < nearest_dist:
                nearest_dist = dist
                nearest = entity

    return nearest


def run_agent(api_key: str):
    """Run the test agent"""
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games first
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join the tsunami game
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Failed to join: {resp.text}")
        return
    print("Joined game!")

    time.sleep(0.5)

    # Agent state
    state = "collect"  # collect, return, deposit, upgrade
    target_brainrot = None
    last_status_time = 0
    last_move_time = time.time()
    last_pos = None

    try:
        while True:
            # Observe
            resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
            if resp.status_code != 200:
                print(f"Observe failed: {resp.status_code}")
                time.sleep(1)
                continue

            obs = resp.json()
            pos = obs["player"]["position"]
            attrs = obs["player"].get("attributes", {})
            entities = obs.get("world", {}).get("entities", [])

            money = attrs.get("Money", 0)
            speed_level = attrs.get("SpeedLevel", 1)
            carried_count = attrs.get("CarriedCount", 0)
            carried_value = attrs.get("CarriedValue", 0)
            passive_income = attrs.get("PassiveIncome", 0)

            now = time.time()

            # Status update every 3 seconds
            if now - last_status_time >= 3.0:
                print(f"[{state.upper()}] pos=({pos[0]:.0f}, {pos[2]:.0f}) money={money:.2f} speed={speed_level} carrying={carried_count} income=${passive_income}/s")
                last_status_time = now

            # Track movement (simple stuck detection)
            if last_pos is None:
                last_pos = pos
            else:
                if distance_xz(pos, last_pos) > 0.5:
                    last_move_time = now
                    last_pos = pos

            # State machine
            if state == "collect":
                # Find nearest brainrot
                brainrot = find_nearest_brainrot(pos, entities)

                if brainrot:
                    dist = distance_xz(pos, brainrot["position"])

                    if dist < COLLECTION_RANGE:
                        # Close enough - collect it
                        resp = requests.post(
                            f"{API_BASE}/games/{GAME_ID}/input",
                            headers=headers,
                            json={"type": "Collect"},
                            timeout=5
                        )
                        if resp.status_code == 200:
                            print(f"  Collected brainrot!")
                    else:
                        # Move toward it
                        resp = requests.post(
                            f"{API_BASE}/games/{GAME_ID}/input",
                            headers=headers,
                            json={"type": "MoveTo", "data": {"position": brainrot["position"]}},
                            timeout=5
                        )
                else:
                    # No brainrots visible - move deeper into collection zone (left side, low X)
                    target_x = random.uniform(COLLECTION_X_MIN + 20, SAFE_ZONE_X_START - 20)
                    target_z = random.uniform(-30, 30)
                    resp = requests.post(
                        f"{API_BASE}/games/{GAME_ID}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": [target_x, pos[1], target_z]}},
                        timeout=5
                    )

                # If at capacity, return to deposit (capacity is currently 1)
                carry_capacity = attrs.get("CarryCapacity", 1)
                if carried_count >= carry_capacity:
                    state = "return"
                    print(f"  Carrying {carried_count}/{carry_capacity} brainrots, returning to deposit...")

            elif state == "return":
                # Move back to safe zone (right side, high X)
                if pos[0] < SAFE_ZONE_X_START:
                    resp = requests.post(
                        f"{API_BASE}/games/{GAME_ID}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": [75, pos[1], 0]}},
                        timeout=5
                    )
                else:
                    state = "deposit"
                # If stuck for >5s, nudge toward base
                if now - last_move_time > 5.0:
                    resp = requests.post(
                        f"{API_BASE}/games/{GAME_ID}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": [pos[0] + 5, pos[1], pos[2]]}},
                        timeout=5
                    )
                    last_move_time = now

            elif state == "deposit":
                # Deposit brainrots (places them on base for passive income)
                resp = requests.post(
                    f"{API_BASE}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "Deposit"},
                    timeout=5
                )
                if resp.status_code == 200:
                    print(f"  Deposited! Brainrots placed on base for passive income.")
                state = "upgrade"

            elif state == "upgrade":
                # Try to buy speed upgrade if affordable
                if int(speed_level) < len(SPEED_COSTS):
                    next_cost = SPEED_COSTS[int(speed_level)]  # speed_level is 1-indexed, costs are 0-indexed for next
                    if money >= next_cost:
                        resp = requests.post(
                            f"{API_BASE}/games/{GAME_ID}/input",
                            headers=headers,
                            json={"type": "BuySpeed"},
                            timeout=5
                        )
                        if resp.status_code == 200:
                            print(f"  Upgraded speed to level {speed_level + 1}!")

                # Go back to collecting
                state = "collect"

            time.sleep(0.2)  # 5 cycles per second

    except KeyboardInterrupt:
        print("\nStopping...")
    finally:
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print("Left game.")


def main():
    parser = argparse.ArgumentParser(description="Test agent for Tsunami Brainrot game")
    parser.add_argument("--api-key", type=str, help="API key (or uses CLAWBLOX_API_KEY env var)")
    args = parser.parse_args()

    api_key = args.api_key or os.getenv("CLAWBLOX_API_KEY")
    if not api_key:
        print("Error: No API key provided. Use --api-key or set CLAWBLOX_API_KEY")
        sys.exit(1)

    print(f"API: {API_BASE}")
    print(f"Game: {GAME_ID}")
    print("-" * 60)

    run_agent(api_key)


if __name__ == "__main__":
    main()
