#!/usr/bin/env python3
"""Stress test: pickup and place 10 blocks near the player, check if any fall through ground."""
import json
import time
import urllib.request

BASE = "http://127.0.0.1:8080"


def req(method, path, session=None, payload=None):
    headers = {}
    body = None
    if payload:
        body = json.dumps(payload).encode()
        headers["Content-Type"] = "application/json"
    if session:
        headers["X-Session"] = session
    r = urllib.request.Request(f"{BASE}{path}", data=body, headers=headers, method=method)
    with urllib.request.urlopen(r) as resp:
        return json.load(resp)


def find_entity(obs, name):
    for e in obs["world"]["entities"]:
        if e["name"] == name:
            return e
    return None


data = req("POST", "/join?name=stress")
session = data["session"]
print("joined")

time.sleep(3)

# Move near block pile
req("POST", "/input", session=session, payload={"type": "MoveTo", "data": {"position": [14, 0, -2]}})
time.sleep(5)

results = []
# Place offsets from player position (within PLACE_RANGE=7)
place_offsets = [
    [3, 2, 0], [-3, 2, 0], [0, 2, 3], [0, 2, -3], [2, 2, 2],
    [-2, 2, -2], [1, 1, 3], [-1, 3, 2], [3, 1, -1], [-3, 2, 1],
]

for i in range(10):
    block_name = f"Block_{i+1}"
    obs = req("GET", "/observe", session=session)
    block = find_entity(obs, block_name)
    if not block:
        print(f"[{i+1}/10] {block_name} not found, skipping")
        results.append(("SKIP", block_name, None))
        continue

    # Move toward block
    req("POST", "/input", session=session, payload={"type": "MoveTo", "data": {"position": block["position"]}})
    time.sleep(3)

    # Refresh position and pickup
    obs = req("GET", "/observe", session=session)
    block = find_entity(obs, block_name)
    req("POST", "/input", session=session, payload={"type": "Pickup", "data": {"position": block["position"]}})
    time.sleep(0.5)

    obs = req("GET", "/observe", session=session)
    if not obs["player"]["attributes"]["HoldingBlock"]:
        print(f"[{i+1}/10] {block_name} pickup failed: {obs['player']['attributes']['LastBlockAction']}")
        results.append(("PICKUP_FAIL", block_name, None))
        continue

    # Place near player
    player_pos = obs["player"]["position"]
    offset = place_offsets[i]
    place_pos = [round(player_pos[0] + offset[0], 2), offset[1], round(player_pos[2] + offset[2], 2)]
    req("POST", "/input", session=session, payload={"type": "Place", "data": {"position": place_pos}})
    time.sleep(0.5)

    obs = req("GET", "/observe", session=session)
    last = obs["player"]["attributes"]["LastBlockAction"]
    if last != "PlaceSuccess":
        print(f"[{i+1}/10] {block_name} place at {place_pos} failed: {last}")
        results.append(("PLACE_FAIL", block_name, None))
        continue

    # Wait for physics
    time.sleep(3)
    obs = req("GET", "/observe", session=session)
    block = find_entity(obs, block_name)
    y = block["position"][1]
    status = "BUG" if y < -1 else "OK"
    print(f"[{i+1}/10] {block_name} placed at {place_pos} -> settled y={y:.2f} [{status}]")
    results.append((status, block_name, y))

print(f"\n=== SUMMARY ===")
bugs = [r for r in results if r[0] == "BUG"]
oks = [r for r in results if r[0] == "OK"]
skips = [r for r in results if r[0] in ("SKIP", "PICKUP_FAIL", "PLACE_FAIL")]
print(f"OK: {len(oks)}, BUG: {len(bugs)}, SKIPPED: {len(skips)}")
for status, name, y in bugs:
    print(f"  {name} fell to y={y:.2f}")
