#!/usr/bin/env python3
"""
Test matchmaking/multi-instance behavior for the Tsunami game.

Verifies that:
1. First 8 players join the same instance
2. 9th player gets a NEW instance (not kicked)
3. All players can observe their games (no one is kicked)

Usage:
    uv run games/tsunami-brainrot/test_matchmaking.py
    uv run games/tsunami-brainrot/test_matchmaking.py --api-base http://localhost:8080/api/v1
"""

import argparse
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
GAME_ID = "0a62727e-b45e-4175-be9f-1070244f8885"  # Tsunami Brainrot (max_players=8)


def register_agent(api_base: str, name: str) -> str | None:
    """Register a new agent and return the API key."""
    try:
        resp = requests.post(
            f"{api_base}/agents/register",
            json={"name": name, "description": "Matchmaking test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
        print(f"  Registration failed: {resp.status_code} {resp.text}")
    except Exception as e:
        print(f"  Registration error: {e}")
    return None


def get_headers(api_key: str) -> dict:
    return {"Authorization": f"Bearer {api_key}"}


def join_game(api_base: str, game_id: str, headers: dict) -> dict | None:
    """Join a game and return the response (includes instance_id)."""
    try:
        resp = requests.post(
            f"{api_base}/games/{game_id}/join",
            headers=headers,
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()
        print(f"  Join failed: {resp.status_code} {resp.text}")
    except Exception as e:
        print(f"  Join error: {e}")
    return None


def leave_game(api_base: str, game_id: str, headers: dict):
    """Leave a game."""
    try:
        requests.post(
            f"{api_base}/games/{game_id}/leave",
            headers=headers,
            timeout=5,
        )
    except:
        pass


def observe(api_base: str, game_id: str, headers: dict) -> dict | None:
    """Get observation for a player."""
    try:
        resp = requests.get(
            f"{api_base}/games/{game_id}/observe",
            headers=headers,
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()
    except:
        pass
    return None


def test_matchmaking(api_base: str, game_id: str, num_players: int = 9):
    """
    Test that:
    1. First 8 players get the same instance
    2. 9th player gets a different instance
    3. All players can observe (no one is kicked)
    """
    print(f"=== Matchmaking Test ===")
    print(f"API: {api_base}")
    print(f"Game: {game_id}")
    print(f"Players: {num_players}")
    print()

    # Register agents
    print(f"Registering {num_players} agents...")
    agents = []
    for i in range(num_players):
        name = f"matchmaking_test_{i}_{random.randint(1000, 9999)}"
        api_key = register_agent(api_base, name)
        if not api_key:
            print(f"FAIL: Could not register agent {i}")
            return False
        agents.append({"name": name, "api_key": api_key, "headers": get_headers(api_key)})
        print(f"  Agent {i}: registered")

    print()

    # Join all agents and track instance_ids
    print("Joining agents to game...")
    instance_ids = []
    for i, agent in enumerate(agents):
        result = join_game(api_base, game_id, agent["headers"])
        if not result:
            print(f"FAIL: Agent {i} could not join")
            # Cleanup
            for a in agents[:i]:
                leave_game(api_base, game_id, a["headers"])
            return False

        instance_id = result.get("instance_id")
        if not instance_id:
            print(f"FAIL: Agent {i} join response missing instance_id")
            print(f"  Response: {result}")
            # Cleanup
            for a in agents[:i+1]:
                leave_game(api_base, game_id, a["headers"])
            return False

        instance_ids.append(instance_id)
        print(f"  Agent {i}: joined instance {instance_id[:8]}...")

        # Small delay to avoid race conditions
        time.sleep(0.1)

    print()

    # Verify instance distribution
    print("Verifying instance distribution...")
    first_8_instances = set(instance_ids[:8])
    ninth_instance = instance_ids[8] if len(instance_ids) > 8 else None

    print(f"  First 8 agents' instances: {first_8_instances}")
    if ninth_instance:
        print(f"  9th agent's instance: {ninth_instance[:8]}...")

    # Check: all first 8 should be in the SAME instance
    if len(first_8_instances) != 1:
        print(f"FAIL: First 8 agents are in {len(first_8_instances)} different instances (expected 1)")
        for a in agents:
            leave_game(api_base, game_id, a["headers"])
        return False
    print(f"  OK: All first 8 agents are in the same instance")

    # Check: 9th agent should be in a DIFFERENT instance
    if ninth_instance:
        first_instance = list(first_8_instances)[0]
        if ninth_instance == first_instance:
            print(f"FAIL: 9th agent is in the same instance as the first 8 (should be different)")
            for a in agents:
                leave_game(api_base, game_id, a["headers"])
            return False
        print(f"  OK: 9th agent is in a different instance")

    print()

    # Verify all agents can observe (no one was kicked)
    print("Verifying all agents can observe (no kicks)...")
    for i, agent in enumerate(agents):
        obs = observe(api_base, game_id, agent["headers"])
        if not obs:
            print(f"FAIL: Agent {i} cannot observe (was probably kicked)")
            for a in agents:
                leave_game(api_base, game_id, a["headers"])
            return False

        # Check observation has player data
        if "player" not in obs:
            print(f"FAIL: Agent {i} observation missing player data")
            for a in agents:
                leave_game(api_base, game_id, a["headers"])
            return False

        print(f"  Agent {i}: can observe, position={obs['player'].get('position', 'N/A')}")

    print()

    # Cleanup
    print("Cleaning up (leaving game)...")
    for i, agent in enumerate(agents):
        leave_game(api_base, game_id, agent["headers"])
        print(f"  Agent {i}: left")

    print()
    print("=== ALL TESTS PASSED ===")
    return True


def main():
    parser = argparse.ArgumentParser(description="Test matchmaking for Tsunami game")
    parser.add_argument(
        "--api-base",
        type=str,
        default=API_BASE,
        help=f"API base URL (default: {API_BASE})",
    )
    parser.add_argument(
        "--game-id",
        type=str,
        default=GAME_ID,
        help=f"Game ID to test (default: {GAME_ID})",
    )
    parser.add_argument(
        "-n", "--num-players",
        type=int,
        default=9,
        help="Number of players to test (default: 9)",
    )
    args = parser.parse_args()

    success = test_matchmaking(args.api_base, args.game_id, args.num_players)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
