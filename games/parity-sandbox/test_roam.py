#!/usr/bin/env python3
"""
Simple roaming test player for Parity Sandbox.

Behavior:
- Resolves game id
- Joins game
- Sends MoveTo waypoints for N seconds
- Leaves game
"""

import argparse
import json
import os
import random
import time
from pathlib import Path

import requests

env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
KEYS_CACHE = Path("/tmp/clawblox_parity_sandbox_keys.json")
DEFAULT_GAME_NAME = "Parity Sandbox"


def load_cached_keys() -> list[str]:
    if KEYS_CACHE.exists():
        try:
            return json.loads(KEYS_CACHE.read_text()).get("keys", [])
        except Exception:
            pass
    return []


def save_cached_keys(keys: list[str]) -> None:
    KEYS_CACHE.write_text(json.dumps({"keys": keys}, indent=2))


def get_api_key(preferred_name: str | None) -> str:
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        return env_key

    cached = load_cached_keys()
    if cached:
        return cached[0]

    names = []
    if preferred_name:
        names.append(preferred_name)
    names.extend([f"parity_roamer_{random.randint(1000, 9999)}" for _ in range(8)])

    for name in names:
        resp = requests.post(
            f"{API_BASE}/agents/register",
            json={"name": name, "description": "Parity sandbox roaming test player"},
            timeout=10,
        )
        if resp.status_code == 200:
            key = resp.json()["agent"]["api_key"]
            save_cached_keys([key])
            return key
        if resp.status_code == 409:
            continue
        raise RuntimeError(f"Registration failed: {resp.status_code} {resp.text}")

    raise RuntimeError("Failed to acquire API key")


def resolve_game_id(headers: dict, explicit_game_id: str | None) -> str:
    if explicit_game_id:
        return explicit_game_id

    local_id_path = Path(__file__).parent / ".clawblox" / "game_id"
    if local_id_path.exists():
        val = local_id_path.read_text().strip()
        if val:
            return val

    resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=10)
    if resp.status_code != 200:
        raise RuntimeError(f"Failed to list games: {resp.status_code} {resp.text}")

    games = resp.json().get("games", [])
    for game in games:
        if game.get("name") == DEFAULT_GAME_NAME:
            return game["id"]

    raise RuntimeError(
        "Could not resolve game id. Deploy first with:\n"
        "  ./scripts/deploy_local_game.sh --game games/parity-sandbox"
    )


def random_waypoint() -> list[float]:
    # Keep waypoints inside the playable baseplate
    return [random.uniform(-30, 30), 6.0, random.uniform(-30, 30)]


def main() -> int:
    parser = argparse.ArgumentParser(description="Parity Sandbox roaming player")
    parser.add_argument("--game-id", help="Game UUID (optional)")
    parser.add_argument("--name", help="Preferred registration name")
    parser.add_argument("--duration", type=float, default=20.0, help="Run time in seconds")
    parser.add_argument("--interval", type=float, default=0.6, help="Seconds between MoveTo calls")
    args = parser.parse_args()

    api_key = get_api_key(args.name)
    headers = {"Authorization": f"Bearer {api_key}"}
    game_id = resolve_game_id(headers, args.game_id)
    print(f"Using game: {game_id}")

    join = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers, timeout=10)
    if join.status_code != 200:
        raise RuntimeError(f"Join failed: {join.status_code} {join.text}")

    print("Joined. Roaming...")
    start = time.time()
    sent = 0
    try:
        while time.time() - start < args.duration:
            waypoint = random_waypoint()
            payload = {"type": "MoveTo", "data": {"position": waypoint}}
            resp = requests.post(
                f"{API_BASE}/games/{game_id}/input",
                headers=headers,
                json=payload,
                timeout=5,
            )
            if resp.status_code != 200:
                raise RuntimeError(f"MoveTo failed: {resp.status_code} {resp.text}")
            sent += 1
            print(f"MoveTo -> ({waypoint[0]:.1f}, {waypoint[1]:.1f}, {waypoint[2]:.1f})")
            time.sleep(args.interval)
    finally:
        leave = requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers, timeout=5)
        if leave.status_code >= 500:
            raise RuntimeError(f"Leave failed: {leave.status_code} {leave.text}")

    print(f"Done. Sent {sent} MoveTo commands in {args.duration:.1f}s.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
