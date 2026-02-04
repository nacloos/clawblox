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
import threading
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
API_BASE_PROD = os.getenv("CLAWBLOX_API_URL_PROD", "")
GAME_ID = "0a62727e-b45e-4175-be9f-1070244f8885"  # Tsunami Brainrot
KEYS_CACHE = Path("/tmp/clawblox_tsunami_keys.json")

# Map constants (800-stud map: X is long axis, Z is short axis)
# Base zone: X >= 350, Collection zones: X < 350
BASE_ZONE_X_START = 350  # X >= 350 is base zone
DEPOSIT_X = 375  # Fallback deposit area center
COLLECTION_X_MIN = -400
COLLECTION_X_MAX = 350
MAP_HALF_WIDTH = 40  # Z from -40 to +40
COLLECTION_RANGE = 5
BASE_SIZE_X = 30  # Fallback base size (X)
BASE_SIZE_Z = 30  # Fallback base size (Z)

# Speed upgrade costs
SPEED_COSTS = [0, 100, 300, 700, 1500, 3000, 6000, 12000, 25000, 50000]


def distance_xz(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def load_cached_keys(api_base: str) -> list[str]:
    if KEYS_CACHE.exists():
        try:
            data = json.loads(KEYS_CACHE.read_text())
            # New cache format: {"by_api": {"https://.../api/v1": ["key1", ...]}}
            by_api = data.get("by_api")
            if isinstance(by_api, dict):
                keys = by_api.get(api_base, [])
                if isinstance(keys, list):
                    return keys
        except:
            pass
    return []


def save_cached_keys(api_base: str, keys: list[str]):
    data = {"by_api": {api_base: keys}}
    if KEYS_CACHE.exists():
        try:
            existing = json.loads(KEYS_CACHE.read_text())
            if isinstance(existing, dict) and isinstance(existing.get("by_api"), dict):
                data["by_api"] = existing["by_api"]
        except:
            pass
    data["by_api"][api_base] = keys
    KEYS_CACHE.write_text(json.dumps(data))


def register_agent(api_base: str, name: str) -> str | None:
    try:
        resp = requests.post(
            f"{api_base}/agents/register",
            json={"name": name, "description": "Tsunami test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def is_valid_api_key(api_base: str, api_key: str) -> bool:
    try:
        resp = requests.get(
            f"{api_base}/agents/me",
            headers={"Authorization": f"Bearer {api_key}"},
            timeout=5,
        )
        return resp.status_code == 200
    except Exception:
        return False


def get_api_keys(api_base: str, num_needed: int, is_prod: bool = False) -> list[str]:
    """Get or register enough API keys for num_needed agents"""
    candidate_keys = []
    env_key = os.getenv("CLAWBLOX_API_KEY_PROD" if is_prod else "CLAWBLOX_API_KEY")
    if env_key:
        candidate_keys.append(env_key)

    for k in load_cached_keys(api_base):
        if k not in candidate_keys:
            candidate_keys.append(k)

    # Filter out stale/invalid keys so local/prod keys do not get mixed.
    keys: list[str] = []
    invalid_count = 0
    for k in candidate_keys:
        if is_valid_api_key(api_base, k):
            keys.append(k)
        else:
            invalid_count += 1
    if invalid_count > 0:
        print(f"Discarded {invalid_count} invalid cached key(s)", flush=True)

    while len(keys) < num_needed:
        name = f"tsunami_agent_{random.randint(1000, 9999)}"
        print(f"Registering {name}...", flush=True)
        key = register_agent(api_base, name)
        if key:
            keys.append(key)
            print(f"  OK: {key[:20]}...", flush=True)

    save_cached_keys(api_base, keys)
    return keys[:num_needed]


def find_nearest_brainrot(pos: list, entities: list) -> tuple[dict | None, int, float | None, float | None]:
    """Find the nearest brainrot from world entities"""
    nearest = None
    nearest_dist = float('inf')
    nearest_by_pos_dist = float('inf')
    count = 0

    for entity in entities:
        if entity.get("name") == "Brainrot":
            # Ignore placed brainrots in the base zone
            if entity["position"][0] >= BASE_ZONE_X_START:
                continue
            count += 1
            dist_xz = distance_xz(pos, entity["position"])
            if dist_xz < nearest_by_pos_dist:
                nearest_by_pos_dist = dist_xz

            dist_attr = None
            if "distance" in entity:
                try:
                    dist_attr = float(entity["distance"])
                except Exception:
                    dist_attr = None

            dist = dist_attr if dist_attr is not None else dist_xz
            if dist < nearest_dist:
                nearest_dist = dist
                nearest = entity

    if nearest is None:
        return None, count, None, None
    return nearest, count, nearest_dist, nearest_by_pos_dist


def run_agent(agent_id: int, api_key: str, api_base: str, stop_event: threading.Event):
    """Run a single test agent"""
    prefix = f"[{agent_id}]"
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games first
    try:
        resp = requests.get(f"{api_base}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{api_base}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join the tsunami game
    resp = requests.post(f"{api_base}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"{prefix} Failed to join: {resp.text}")
        return
    print(f"{prefix} Joined game!")

    time.sleep(0.5)

    # Agent state
    state = "collect"  # collect, return, deposit, upgrade
    target_brainrot = None
    last_status_time = 0
    last_move_time = time.time()
    last_pos = None

    try:
        while not stop_event.is_set():
            # Observe
            resp = requests.get(f"{api_base}/games/{GAME_ID}/observe", headers=headers, timeout=5)
            if resp.status_code != 200:
                print(f"{prefix} Observe failed: {resp.status_code}")
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
            base_center_x = attrs.get("BaseCenterX", DEPOSIT_X)
            base_center_z = attrs.get("BaseCenterZ", 0)
            base_size_x = attrs.get("BaseSizeX", BASE_SIZE_X)
            base_size_z = attrs.get("BaseSizeZ", BASE_SIZE_Z)

            now = time.time()

            # Status update every 3 seconds
            should_log = now - last_status_time >= 3.0
            if should_log:
                entity_count = len(entities)
                print(f"{prefix} [{state.upper()}] pos=({pos[0]:.0f}, {pos[2]:.0f}) money={money:.2f} speed={speed_level} carrying={carried_count} income=${passive_income}/s base=({base_center_x:.0f},{base_center_z:.0f}) entities={entity_count}")
                last_status_time = now

            if state == "collect" and should_log:
                if entities:
                    sample = []
                    for e in entities[:5]:
                        sample.append(f"{e.get('name')}@({e['position'][0]:.0f},{e['position'][2]:.0f})")
                    print(f"{prefix} Entities sample: {', '.join(sample)}")

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
                brainrot, brainrot_count, nearest_dist, nearest_by_pos_dist = find_nearest_brainrot(pos, entities)

                if brainrot:
                    dist = nearest_dist if nearest_dist is not None else distance_xz(pos, brainrot["position"])

                    if dist < COLLECTION_RANGE:
                        # Close enough - collect it
                        resp = requests.post(
                            f"{api_base}/games/{GAME_ID}/input",
                            headers=headers,
                            json={"type": "Collect"},
                            timeout=5
                        )
                        if resp.status_code == 200:
                            print(f"{prefix} Collected brainrot!")
                    else:
                        # Move toward it
                        resp = requests.post(
                            f"{api_base}/games/{GAME_ID}/input",
                            headers=headers,
                            json={"type": "MoveTo", "data": {"position": brainrot["position"]}},
                            timeout=5
                        )
                        if should_log:
                            extra = f" pos_dist={nearest_by_pos_dist:.1f}" if nearest_by_pos_dist is not None else ""
                            print(f"{prefix} MoveTo brainrot dist={dist:.1f}{extra} status={resp.status_code}")
                else:
                    # No brainrots visible - move deeper into collection zone (left side, low X)
                    if should_log:
                        print(f"{prefix} No brainrots visible (count={brainrot_count}), roaming...")
                    target_x = random.uniform(COLLECTION_X_MIN + 20, BASE_ZONE_X_START - 20)
                    target_z = random.uniform(-30, 30)
                    resp = requests.post(
                        f"{api_base}/games/{GAME_ID}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": [target_x, pos[1], target_z]}},
                        timeout=5
                    )
                    if should_log:
                        print(f"{prefix} Roam MoveTo target=({target_x:.0f},{target_z:.0f}) status={resp.status_code}")

                # If at capacity, return to deposit (capacity is currently 1)
                carry_capacity = attrs.get("CarryCapacity", 1)
                if carried_count >= carry_capacity:
                    state = "return"
                    print(f"{prefix} Carrying {carried_count}/{carry_capacity}, returning...")

            elif state == "return":
                # Move back to deposit area (X=360-390, Z near 0)
                # Need to be at player's base bounds
                dx = abs(pos[0] - base_center_x)
                dz = abs(pos[2] - base_center_z)
                if dx > base_size_x / 2 or dz > base_size_z / 2:
                    resp = requests.post(
                        f"{api_base}/games/{GAME_ID}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": [base_center_x, pos[1], base_center_z]}},
                        timeout=5
                    )
                    if now - last_status_time >= 3.0:
                        print(f"{prefix} RETURN move: dx={dx:.1f} dz={dz:.1f} target=({base_center_x:.0f},{base_center_z:.0f}) status={resp.status_code}")
                else:
                    if now - last_status_time >= 3.0:
                        print(f"{prefix} RETURN reached base: dx={dx:.1f} dz={dz:.1f} -> deposit")
                    state = "deposit"
                # If stuck for >5s, nudge toward base
                if now - last_move_time > 5.0:
                    resp = requests.post(
                        f"{api_base}/games/{GAME_ID}/input",
                        headers=headers,
                        json={"type": "MoveTo", "data": {"position": [pos[0] + 10, pos[1], pos[2]]}},
                        timeout=5
                    )
                    last_move_time = now

            elif state == "deposit":
                # Deposit brainrots (places them on base for passive income)
                resp = requests.post(
                    f"{api_base}/games/{GAME_ID}/input",
                    headers=headers,
                    json={"type": "Deposit"},
                    timeout=5
                )
                if resp.status_code == 200:
                    print(f"{prefix} Deposited!")
                else:
                    print(f"{prefix} Deposit failed: {resp.status_code} {resp.text}")
                state = "upgrade"

            elif state == "upgrade":
                # Try to buy speed upgrade if affordable
                if int(speed_level) < len(SPEED_COSTS):
                    next_cost = SPEED_COSTS[int(speed_level)]  # speed_level is 1-indexed, costs are 0-indexed for next
                    if money >= next_cost:
                        resp = requests.post(
                            f"{api_base}/games/{GAME_ID}/input",
                            headers=headers,
                            json={"type": "BuySpeed"},
                            timeout=5
                        )
                        if resp.status_code == 200:
                            print(f"{prefix} Speed upgraded to level {speed_level + 1}!")

                # Go back to collecting
                state = "collect"

            time.sleep(0.2)  # 5 cycles per second

    finally:
        requests.post(f"{api_base}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print(f"{prefix} Left game.")


def main():
    parser = argparse.ArgumentParser(description="Test agent for Tsunami Brainrot game")
    parser.add_argument("-n", "--num-agents", type=int, default=1, help="Number of agents")
    parser.add_argument("--api-key", type=str, help="API key (or uses env var)")
    parser.add_argument(
        "--prod",
        action="store_true",
        help="Use production API base (CLAWBLOX_API_URL_PROD or explicit --api-base)",
    )
    parser.add_argument(
        "--api-base",
        type=str,
        default=None,
        help="Override API base URL (e.g., https://host/api/v1)",
    )
    args = parser.parse_args()

    api_base = API_BASE
    if args.api_base:
        api_base = args.api_base
    elif args.prod:
        if API_BASE_PROD:
            api_base = API_BASE_PROD
        else:
            print("Error: --prod set but CLAWBLOX_API_URL_PROD is not configured")
            sys.exit(1)

    print(f"API: {api_base}")
    print(f"Game: {GAME_ID}")
    print(f"Agents: {args.num_agents}")
    print("-" * 60)

    # Get API keys
    api_keys = get_api_keys(api_base, args.num_agents, is_prod=args.prod)
    print(f"Got {len(api_keys)} API key(s)")

    # Start agents
    stop_event = threading.Event()
    threads = []

    for i in range(args.num_agents):
        t = threading.Thread(target=run_agent, args=(i, api_keys[i], api_base, stop_event))
        t.daemon = True
        t.start()
        threads.append(t)
        time.sleep(0.3)  # Stagger joins

    # Run until Ctrl+C
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\nStopping...")
    finally:
        stop_event.set()
        for t in threads:
            t.join(timeout=2)


if __name__ == "__main__":
    main()
