#!/usr/bin/env python3
"""
Test agent for Physics Test game.
Walks to each test zone and logs observations to verify physics behavior.
"""

import argparse
import json
import os
import random
import sys
import time
from pathlib import Path

import requests

# Load .env file if present (project root is ../../)
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
GAME_ID = "f47ac10b-58cc-4372-a567-0e02b2c3d479"  # Physics Test
KEYS_CACHE = Path("/tmp/clawblox_physics_keys.json")

# Test movement tolerances (XZ distance in studs)
RESET_BUTTON_REACH_THRESHOLD = 3.0
TRIGGER_REACH_THRESHOLD = 2.0
KILL_APPROACH_THRESHOLD = 6.5
JUMP_ZONE_REACH_THRESHOLD = 4.5
JUMP_APPROACH_THRESHOLD = 1.5
JUMP_PLATFORM_REACH_XZ = 2.2
JUMP_ATTEMPT_TIMEOUT = 6.0
JUMP_TRIGGER_DIST = 10.0
JUMP_OVERTRAVEL_X = 44.0


def distance_xz(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


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
            json={"name": name, "description": "Physics test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def get_api_key() -> str:
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        return env_key

    keys = load_cached_keys()
    if keys:
        return keys[0]

    name = f"physics_agent_{random.randint(1000, 9999)}"
    print(f"Registering {name}...")
    key = register_agent(name)
    if key:
        save_cached_keys([key])
        return key

    print("Failed to get API key")
    sys.exit(1)


def observe(headers: dict) -> dict | None:
    try:
        resp = requests.get(f"{API_BASE}/games/{GAME_ID}/observe", headers=headers, timeout=5)
        if resp.status_code == 200:
            return resp.json()
        print(f"Observe failed: {resp.status_code} {resp.text}")
    except Exception as e:
        print(f"Observe error: {e}")
    return None


def move_to(headers: dict, pos: list) -> requests.Response | None:
    try:
        return requests.post(
            f"{API_BASE}/games/{GAME_ID}/input",
            headers=headers,
            json={"type": "MoveTo", "data": {"position": pos}},
            timeout=5,
        )
    except Exception as e:
        print(f"MoveTo error: {e}")
        return None


def send_input(headers: dict, input_type: str, data: dict | None = None):
    payload = {"type": input_type}
    if data:
        payload["data"] = data
    try:
        resp = requests.post(
            f"{API_BASE}/games/{GAME_ID}/input",
            headers=headers,
            json=payload,
            timeout=5,
        )
        if resp.status_code != 200:
            print(f"Input {input_type} failed: {resp.status_code} {resp.text}")
    except Exception as e:
        print(f"Input {input_type} error: {e}")


def wait_until_near(headers: dict, target: list, threshold: float = 3.0, timeout: float = 15.0) -> list | None:
    """Move to target and wait until close. Returns final position or None on timeout."""
    start = time.time()
    best_dist = float("inf")
    best_pos = None
    last_pos = None
    while time.time() - start < timeout:
        obs = observe(headers)
        if not obs:
            time.sleep(0.3)
            continue
        pos = obs["player"]["position"]
        last_pos = pos
        dist = distance_xz(pos, target)
        if dist < best_dist:
            best_dist = dist
            best_pos = pos
        if dist < threshold:
            return pos
        move_target = [target[0], pos[1], target[2]]
        resp = move_to(headers, move_target)
        if resp is None:
            print(f"  MoveTo request error while heading to {target}")
        elif resp.status_code != 200:
            print(f"  MoveTo failed ({resp.status_code}) while heading to {target}: {resp.text}")
        time.sleep(0.3)
    print(
        f"  Timeout heading to {target}: "
        f"best_dist_xz={best_dist:.2f} best_pos={best_pos} last_pos={last_pos}"
    )
    return None


def find_entities_by_name(obs: dict, name: str) -> list[dict]:
    entities = obs.get("world", {}).get("entities", [])
    return [e for e in entities if e.get("name") == name]

def get_entity_attr(entity: dict, key: str, default=None):
    attrs = entity.get("attributes") or {}
    return attrs.get(key, default)


# ---------------------------------------------------------------------------
# Test routines — each walks to a zone and checks observations
# ---------------------------------------------------------------------------

def test_rotation_sync(headers: dict):
    """Test 1: Walk near the rotating wall and check it has rotation data."""
    print("\n--- TEST 1: Rotation Sync (X=20, Z=-20) ---")
    target = [20, 3, -15]  # Approach from safe side

    pos = wait_until_near(headers, target)
    if not pos:
        print("  SKIP: Could not reach rotation test zone")
        return

    # Observe a few times to see the wall rotating
    for i in range(5):
        obs = observe(headers)
        if obs:
            walls = find_entities_by_name(obs, "RotatingWall")
            if walls:
                w = walls[0]
                rot = w.get("rotation", w.get("cframe", {}).get("rotation"))
                print(f"  Frame {i}: RotatingWall pos={w['position'][:2]} rotation={rot}")
            else:
                print(f"  Frame {i}: RotatingWall not found in entities")
        time.sleep(0.5)

    print("  CHECK: Rotation values should change each frame (Phase 1)")


def test_property_changes(headers: dict):
    """Test 2: Observe growing part, toggling part, delayed drop."""
    print("\n--- TEST 2: Property Changes (X=-20) ---")
    target = [-15, 3, -15]

    pos = wait_until_near(headers, target)
    if not pos:
        print("  SKIP: Could not reach property test zone")
        return

    for i in range(10):
        obs = observe(headers)
        if obs:
            growing = find_entities_by_name(obs, "GrowingPart")
            toggle = find_entities_by_name(obs, "TogglePart")
            drop = find_entities_by_name(obs, "DelayedDrop")

            parts = []
            if growing:
                s = growing[0].get("size", "?")
                parts.append(f"Growing size={s}")
            if toggle:
                t = toggle[0].get("transparency", "?")
                parts.append(f"Toggle transp={t}")
            if drop:
                p = drop[0].get("position", "?")
                parts.append(f"Drop pos={p}")

            print(f"  Frame {i}: {' | '.join(parts) if parts else 'no parts found'}")
        time.sleep(0.5)

    print("  CHECK: Size should change, transparency should toggle, drop should fall (Phase 2)")


def test_shapes(headers: dict):
    """Test 3: Check that ball/cylinder/wedge parts exist with correct shapes.
    Also verifies collider shapes via slope interaction:
      - SlopeBall dropped onto wedge ramp should slide laterally (+X)
      - SlopeBlock dropped onto wedge ramp should also slide
    """
    print("\n--- TEST 3: Part Shapes (Z=30) ---")

    # Walk to reset button to respawn slope objects
    reset_btn = [20, 3, 24]
    print("  Walking to reset button...")
    pos = wait_until_near(headers, reset_btn, threshold=RESET_BUTTON_REACH_THRESHOLD)
    if not pos:
        print("  SKIP: Could not reach reset button")
        return
    print("  Reset triggered! Waiting for objects to spawn and interact...")
    time.sleep(0.5)

    # Check shape properties
    obs = observe(headers)
    if obs:
        for name in ["TestBall", "TestCylinder", "TestWedge"]:
            parts = find_entities_by_name(obs, name)
            if parts:
                p = parts[0]
                shape = p.get("shape", "?")
                pos = p.get("position", "?")
                print(f"  {name}: shape={shape} pos={pos}")
            else:
                print(f"  {name}: NOT FOUND")

    # Observe SlopeBall and SlopeBlock right after reset to catch them moving
    print("\n  Slope interaction proof (observing over 3 seconds):")
    positions = {"SlopeBall": [], "SlopeBlock": []}
    for i in range(10):
        obs = observe(headers)
        if obs:
            for name in ["SlopeBall", "SlopeBlock"]:
                parts = find_entities_by_name(obs, name)
                if parts:
                    p = parts[0]["position"]
                    positions[name].append(p)
                    print(f"    Frame {i}: {name} pos=({p[0]:.2f}, {p[1]:.2f}, {p[2]:.2f})")
                else:
                    print(f"    Frame {i}: {name} NOT FOUND")
        time.sleep(0.3)

    # Verify: objects on wedge slope should have moved in X from their spawn at X=19
    spawn_x = 19.0
    print("\n  Results:")
    for name in ["SlopeBall", "SlopeBlock"]:
        pts = positions[name]
        if len(pts) >= 2:
            total_dx = pts[-1][0] - spawn_x
            obs_dx = pts[-1][0] - pts[0][0]
            still_moving = abs(obs_dx) > 0.5
            if total_dx > 1.0:
                status = "still rolling" if still_moving else "came to rest"
                print(f"  PASS {name}: displaced {total_dx:+.2f} from spawn ({status})")
            else:
                print(f"  FAIL {name}: total displacement {total_dx:+.2f} from spawn — wedge may be a cuboid")
        else:
            print(f"  SKIP {name}: not enough position samples")


def test_touched_events(headers: dict):
    """Test 4: Walk through trigger zone and kill zone."""
    print("\n--- TEST 4: Touched Events (Z=-30) ---")

    # Walk through the trigger zone
    print("  Walking through trigger zone...")
    target = [0, 3, -30]
    pos = wait_until_near(headers, target, threshold=TRIGGER_REACH_THRESHOLD)
    if pos:
        print(f"  Reached trigger at xz=({pos[0]:.2f}, {pos[2]:.2f})")
    else:
        print("  SKIP: Could not reach trigger zone")

    time.sleep(1)

    # Walk through kill zone
    print("  Walking through kill zone...")
    target = [-10, 3, -30]
    spawn_xz = [0, 0]
    saw_kill_approach = False
    reached_center = False
    respawned = False
    best_dist = float("inf")

    start = time.time()
    while time.time() - start < 15:
        obs = observe(headers)
        if not obs:
            time.sleep(0.3)
            continue

        pos = obs["player"]["position"]
        dist_to_kill = distance_xz(pos, target)
        best_dist = min(best_dist, dist_to_kill)
        dist_to_spawn = distance_xz(pos, [spawn_xz[0], pos[1], spawn_xz[1]])

        # Character capsule touches before center-distance gets very small.
        if dist_to_kill < KILL_APPROACH_THRESHOLD:
            saw_kill_approach = True
        if dist_to_kill < 2.0:
            reached_center = True
        if saw_kill_approach and dist_to_spawn < 3.0 and dist_to_kill > 20.0:
            respawned = True
            print(f"  Respawned to xz=({pos[0]:.2f}, {pos[2]:.2f}) after kill-zone approach")
            break

        move_target = [target[0], pos[1], target[2]]
        move_to(headers, move_target)
        time.sleep(0.3)

    if reached_center:
        print("  Reached kill-zone center before respawn")
    elif not respawned:
        print(f"  SKIP: Could not confirm kill-zone touch (best_dist_xz={best_dist:.2f})")

    print("  CHECK: Trigger should fire, kill zone should respawn (Phase 4)")


def test_jump(headers: dict):
    """Test 5: Sequential jump climb: low -> medium -> high."""
    print("\n--- TEST 5: Jump Platforms (X=40) ---")
    pos = wait_until_near(headers, [33, 3, 0], threshold=JUMP_APPROACH_THRESHOLD)
    if not pos:
        print("  SKIP: Could not reach jump zone")
        return

    results = []
    for height_label, platform_center_y, z in [
        ("Low (Y=2)", 2.0, 0.0),
        ("Med (Y=5)", 5.0, 10.0),
        ("High (Y=8)", 8.0, 20.0),
    ]:
        print(f"  Jumping to {height_label}...")
        required_hrp_y = platform_center_y + 2.2
        start = time.time()
        success = False
        last_pos = None
        jump_sent = False

        while time.time() - start < JUMP_ATTEMPT_TIMEOUT:
            obs = observe(headers)
            if not obs:
                time.sleep(0.2)
                continue

            p = obs["player"]["position"]
            last_pos = p
            dist = distance_xz(p, [40.0, p[1], z])
            move_to(headers, [42.0, 10.0, z])

            # One early jump per platform attempt.
            if dist < 9.0 and not jump_sent:
                send_input(headers, "Jump")
                jump_sent = True

            on_platform_xz = abs(p[0] - 40.0) <= 3.1 and abs(p[2] - z) <= 3.1
            if on_platform_xz and p[1] >= required_hrp_y:
                success = True
                break

            time.sleep(0.2)

        if success:
            print(f"  PASS {height_label}: reached top (pos={last_pos})")
        else:
            print(f"  FAIL {height_label}: last_pos={last_pos}")
        results.append((height_label, success))

    passed = sum(1 for _, ok in results if ok)
    print(f"  RESULT: {passed}/3 platforms reached")
    print("  CHECK: Low/med should be reachable; high depends on exact jump tuning")


def test_jump_simple(headers: dict):
    """Test 7: Minimal jump sanity check (flat -> low platform only)."""
    print("\n--- TEST 7: Jump Simple (Low Platform Only) ---")

    # Stage on flat ground before the low platform.
    stage = [30.0, 3.0, 0.0]
    pos = wait_until_near(headers, stage, threshold=1.0, timeout=10.0)
    if not pos:
        print("  SKIP: Could not reach staging point")
        return

    # Single objective: reach low platform top at (40, 2, 0).
    required_hrp_y = 4.2  # platform top (~2.5) + standing HRP offset margin
    start = time.time()
    success = False
    best_y = -999.0
    best_dist = float("inf")
    last_pos = None
    jump_sent = False

    while time.time() - start < 6.0:
        obs = observe(headers)
        if not obs:
            time.sleep(0.2)
            continue

        p = obs["player"]["position"]
        last_pos = p
        dist = distance_xz(p, [40.0, p[1], 0.0])
        best_dist = min(best_dist, dist)
        best_y = max(best_y, p[1])

        # Aim slightly beyond platform face to avoid stopping on its edge.
        move_x = 43.0 if dist < 10.0 else 39.0
        move_to(headers, [move_x, 10.0, 0.0])

        if dist < 9.0 and not jump_sent:
            send_input(headers, "Jump")
            jump_sent = True

        if dist <= 2.0 and p[1] >= required_hrp_y:
            success = True
            break

        time.sleep(0.2)

    if success:
        print(f"  PASS: landed on low platform (pos={last_pos})")
    else:
        print(f"  FAIL: best_y={best_y:.2f} best_dist_xz={best_dist:.2f} last_pos={last_pos}")


def test_kinematic_push(headers: dict):
    """Test 6: Stand in front of pusher, ride elevator, stand near spinner."""
    print("\n--- TEST 6: Kinematic Push (X=-40) ---")

    # Stand in front of horizontal pusher
    print("  Standing in front of pusher...")
    target = [-30, 3, 20]
    pos = wait_until_near(headers, target)
    if pos:
        # Wait and see if we get pushed
        for i in range(5):
            obs = observe(headers)
            if obs:
                p = obs["player"]["position"]
                print(f"  Pusher frame {i}: player=({p[0]:.1f}, {p[1]:.1f}, {p[2]:.1f})")
            time.sleep(0.5)

    # Stand on elevator
    print("  Standing on elevator...")
    target = [-40, 3, 0]
    pos = wait_until_near(headers, target)
    if pos:
        for i in range(8):
            obs = observe(headers)
            if obs:
                p = obs["player"]["position"]
                elevators = find_entities_by_name(obs, "Elevator")
                elev_y = elevators[0]["position"][1] if elevators else "?"
                print(f"  Elevator frame {i}: player_y={p[1]:.1f} elevator_y={elev_y}")
            time.sleep(0.5)

    # Stand near spinner and verify it actually pushes us
    print("  Standing near spinner...")
    target = [-34, 3, -20]
    pos = wait_until_near(headers, target)
    if pos:
        # Stop path-following so collision response isn't masked by MoveTo corrections.
        send_input(headers, "Stop")
        time.sleep(0.2)

        start_xz = None
        max_disp = 0.0
        for i in range(12):
            obs = observe(headers)
            if obs:
                p = obs["player"]["position"]
                if start_xz is None:
                    start_xz = [p[0], p[2]]
                else:
                    disp = distance_xz([p[0], 0, p[2]], [start_xz[0], 0, start_xz[1]])
                    max_disp = max(max_disp, disp)
                spinners = find_entities_by_name(obs, "Spinner")
                spin_rot = spinners[0].get("rotation", "?") if spinners else "?"
                print(f"  Spinner frame {i}: player=({p[0]:.1f}, {p[2]:.1f}) rotation={spin_rot}")
            time.sleep(0.5)

        if max_disp > 1.0:
            print(f"  PASS Spinner: pushed player by {max_disp:.2f} studs")
        else:
            print(f"  FAIL Spinner: no meaningful push observed (max_disp={max_disp:.2f})")

    print("  CHECK: Pusher should move player, elevator should lift, spinner should sweep (Phase 6)")

def test_raycast_parity(headers: dict):
    """Test 8: Verify rotated thin-part raycast passes via published status attributes."""
    print("\n--- TEST 8: Raycast Parity (X=60, Z=-20) ---")

    target = [60, 3, -20]
    pos = wait_until_near(headers, target, threshold=6.5, timeout=20.0)
    if not pos:
        print("  SKIP: Could not reach raycast parity zone")
        return

    passes = 0
    samples = 0
    for i in range(8):
        obs = observe(headers)
        if not obs:
            time.sleep(0.3)
            continue

        status_parts = find_entities_by_name(obs, "RaycastStatus")
        thin_bar = find_entities_by_name(obs, "RaycastThinBar")
        if not status_parts:
            print(f"  Frame {i}: RaycastStatus not found")
            time.sleep(0.3)
            continue

        status = status_parts[0]
        hit_name = get_entity_attr(status, "RaycastHitName", "missing")
        hit_dist = get_entity_attr(status, "RaycastDistance", -1)
        passed = bool(get_entity_attr(status, "RaycastPass", False))
        bar_rot = thin_bar[0].get("rotation") if thin_bar else None

        samples += 1
        passes += 1 if passed else 0
        print(
            f"  Frame {i}: hit={hit_name} dist={hit_dist} pass={passed} "
            f"bar_rot={'present' if bar_rot else 'missing'}"
        )
        time.sleep(0.3)

    if samples == 0:
        print("  FAIL: no raycast status samples collected")
    elif passes == samples:
        print(f"  PASS: raycast parity stable ({passes}/{samples} passing samples)")
    else:
        print(f"  FAIL: raycast parity unstable ({passes}/{samples} passing samples)")


def test_overlap_parity(headers: dict):
    """Test 9: Verify GetPartsInPart + OverlapParams parity via published status attributes."""
    print("\n--- TEST 9: GetPartsInPart Parity (X=80, Z=-20) ---")

    target = [80, 3, -20]
    pos = wait_until_near(headers, target, threshold=6.5, timeout=20.0)
    if not pos:
        print("  SKIP: Could not reach overlap parity zone")
        return

    passes = 0
    samples = 0
    for i in range(8):
        obs = observe(headers)
        if not obs:
            time.sleep(0.3)
            continue

        status_parts = find_entities_by_name(obs, "OverlapStatus")
        if not status_parts:
            print(f"  Frame {i}: OverlapStatus not found")
            time.sleep(0.3)
            continue

        status = status_parts[0]
        has_default = bool(get_entity_attr(status, "OverlapHasDefault", False))
        has_no_query = bool(get_entity_attr(status, "OverlapHasNoQuery", True))
        has_red_solid = bool(get_entity_attr(status, "OverlapHasRedSolid", False))
        has_red_trigger = bool(get_entity_attr(status, "OverlapHasRedTrigger", True))
        passed = bool(get_entity_attr(status, "OverlapPass", False))

        samples += 1
        passes += 1 if passed else 0
        print(
            f"  Frame {i}: pass={passed} default={has_default} noQuery={has_no_query} "
            f"redSolid={has_red_solid} redTrigger={has_red_trigger}"
        )
        time.sleep(0.3)

    if samples == 0:
        print("  FAIL: no overlap status samples collected")
    elif passes == samples:
        print(f"  PASS: overlap parity stable ({passes}/{samples} passing samples)")
    else:
        print(f"  FAIL: overlap parity unstable ({passes}/{samples} passing samples)")


# ---------------------------------------------------------------------------
# Test registry
# ---------------------------------------------------------------------------

TESTS = {
    "1": ("rotation", test_rotation_sync),
    "2": ("properties", test_property_changes),
    "3": ("shapes", test_shapes),
    "4": ("touched", test_touched_events),
    "5": ("jump", test_jump),
    "6": ("kinematic", test_kinematic_push),
    "7": ("jump_simple", test_jump_simple),
    "8": ("raycast", test_raycast_parity),
    "9": ("overlap", test_overlap_parity),
}

# Build reverse lookup: name -> (number, func)
_NAME_TO_TEST = {name: (num, func) for num, (name, func) in TESTS.items()}


def resolve_tests(args: list[str]) -> list[tuple[str, str, callable]]:
    """Resolve CLI args to list of (number, name, func) tuples."""
    if not args:
        return [(num, name, func) for num, (name, func) in TESTS.items()]

    result = []
    for arg in args:
        if arg in TESTS:
            num = arg
            name, func = TESTS[num]
            result.append((num, name, func))
        elif arg in _NAME_TO_TEST:
            num, func = _NAME_TO_TEST[arg]
            result.append((num, arg, func))
        else:
            print(f"Unknown test: {arg!r}")
            print(f"Available: {', '.join(f'{n}={name}' for n, (name, _) in TESTS.items())}")
            sys.exit(1)
    return result


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Physics test player")
    parser.add_argument(
        "tests",
        nargs="*",
        help="Tests to run (by number or name). If omitted, runs all.",
    )
    args = parser.parse_args()

    selected = resolve_tests(args.tests)

    print("=" * 60)
    print("Physics Test Player")
    print(f"API: {API_BASE}")
    print(f"Game: {GAME_ID}")
    print(f"Tests: {', '.join(f'{num}={name}' for num, name, _ in selected)}")
    print("=" * 60)

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
    print(f"Joining game {GAME_ID}...")
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"Failed to join: {resp.text}")
        sys.exit(1)
    print("Joined!")
    time.sleep(1)

    try:
        for _num, _name, func in selected:
            func(headers)

        print("\n" + "=" * 60)
        print("All selected tests completed. Review output above.")
        print("=" * 60)
    except KeyboardInterrupt:
        print("\nInterrupted.")
    finally:
        requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
        print("Left game.")


if __name__ == "__main__":
    main()
