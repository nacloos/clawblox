#!/usr/bin/env python3
"""Simple movement test to verify character controller works."""

import requests
import time
import random

API_BASE = "http://localhost:8080/api/v1"
GAME_ID = "26c869ee-da7b-48a4-a198-3daa870ef652"  # Flat Test game - no obstacles

def main():
    # Register agent
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": f"test_movement_{random.randint(1000,9999)}", "description": "Movement test"},
        timeout=10
    )
    if resp.status_code != 200:
        print(f"Registration failed: {resp.text}")
        return

    api_key = resp.json()["agent"]["api_key"]
    headers = {"Authorization": f"Bearer {api_key}"}
    print(f"Registered with key: {api_key[:20]}...")

    # Join game
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Join failed: {resp.text}")
        return
    print("Joined game!")
    time.sleep(1)

    # Get initial position
    resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Observe failed: {resp.text}")
        return

    obs = resp.json()
    pos = obs["player"]["position"]
    print(f"Initial position: ({pos[0]:.2f}, {pos[1]:.2f}, {pos[2]:.2f})")

    # Test movement in +X direction
    target = [pos[0] + 20, pos[1], pos[2]]
    print(f"\n=== Moving +X to ({target[0]:.2f}, {target[1]:.2f}, {target[2]:.2f}) ===")
    resp = requests.post(
        f"{API_BASE}/games/{GAME_ID}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": target}},
        timeout=5
    )
    print(f"MoveTo response: {resp.status_code}")

    # Track position for 5 seconds
    for i in range(10):
        time.sleep(0.5)
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        if resp.status_code == 200:
            pos = resp.json()["player"]["position"]
            print(f"  Frame {i}: pos=({pos[0]:.2f}, {pos[1]:.2f}, {pos[2]:.2f})")

    # Test movement in -X direction (reversal)
    new_target = [pos[0] - 20, pos[1], pos[2]]
    print(f"\n=== Moving -X to ({new_target[0]:.2f}, {new_target[1]:.2f}, {new_target[2]:.2f}) ===")
    resp = requests.post(
        f"{API_BASE}/games/{GAME_ID}/input",
        headers=headers,
        json={"type": "MoveTo", "data": {"position": new_target}},
        timeout=5
    )
    print(f"MoveTo response: {resp.status_code}")

    # Track position for 5 seconds
    for i in range(10):
        time.sleep(0.5)
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        if resp.status_code == 200:
            pos = resp.json()["player"]["position"]
            print(f"  Frame {i}: pos=({pos[0]:.2f}, {pos[1]:.2f}, {pos[2]:.2f})")

    # Leave game
    requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
    print("\nLeft game. Test complete!")

if __name__ == "__main__":
    main()
