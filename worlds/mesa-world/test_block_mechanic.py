#!/usr/bin/env python3

import argparse
import json
import math
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Reproduce the mesa-world block pickup/place flow against a running Clawblox server."
    )
    parser.add_argument(
        "--base-url",
        default="http://127.0.0.1:8080",
        help="Base URL for the local Clawblox world server.",
    )
    parser.add_argument(
        "--name",
        default="block-test",
        help="Player name to use when joining the world.",
    )
    parser.add_argument(
        "--move-target",
        nargs=3,
        type=float,
        default=[12.0, 0.0, -2.0],
        metavar=("X", "Y", "Z"),
        help="Position to walk toward before attempting pickup.",
    )
    parser.add_argument(
        "--pickup-target",
        nargs=3,
        type=float,
        default=[12.0, 1.0, -2.0],
        metavar=("X", "Y", "Z"),
        help="World position passed to the Pickup action.",
    )
    parser.add_argument(
        "--place-target",
        nargs=3,
        type=float,
        default=[10.0, 1.0, 2.0],
        metavar=("X", "Y", "Z"),
        help="World position passed to the Place action.",
    )
    parser.add_argument(
        "--move-timeout",
        type=float,
        default=12.0,
        help="Seconds to wait for the player to reach the move target.",
    )
    parser.add_argument(
        "--poll-interval",
        type=float,
        default=0.5,
        help="Seconds between observation polls.",
    )
    return parser.parse_args()


def request_json(
    method: str,
    url: str,
    *,
    session: str | None = None,
    payload: dict | None = None,
) -> dict:
    body = None
    headers = {}
    if payload is not None:
        body = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    if session:
        headers["X-Session"] = session

    req = urllib.request.Request(url, data=body, headers=headers, method=method)
    with urllib.request.urlopen(req) as response:
        return json.load(response)


def join_game(base_url: str, name: str) -> tuple[str, str]:
    url = f"{base_url}/join?{urllib.parse.urlencode({'name': name})}"
    data = request_json("POST", url)
    return data["session"], data["agent_id"]


def observe(base_url: str, session: str) -> dict:
    return request_json("GET", f"{base_url}/observe", session=session)


def send_input(base_url: str, session: str, input_type: str, data: dict) -> dict:
    return request_json(
        "POST",
        f"{base_url}/input",
        session=session,
        payload={"type": input_type, "data": data},
    )


def distance(a: list[float], b: list[float]) -> float:
    return math.sqrt(sum((ax - bx) ** 2 for ax, bx in zip(a, b)))


def find_entity(obs: dict, name: str) -> dict | None:
    for entity in obs["world"]["entities"]:
        if entity["name"] == name:
            return entity
    return None


def wait_for_position(
    base_url: str,
    session: str,
    target: list[float],
    timeout_s: float,
    poll_interval_s: float,
) -> dict:
    deadline = time.time() + timeout_s
    last_obs = observe(base_url, session)
    while time.time() < deadline:
        pos = last_obs["player"]["position"]
        if abs(pos[0] - target[0]) <= 2.5 and abs(pos[2] - target[2]) <= 2.5:
            return last_obs
        time.sleep(poll_interval_s)
        last_obs = observe(base_url, session)
    raise AssertionError(
        f"Timed out waiting for player to reach {target}. Last position: {last_obs['player']['position']}"
    )


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> int:
    args = parse_args()

    try:
        session, agent_id = join_game(args.base_url, args.name)
        print(f"joined agent_id={agent_id} session={session}")

        initial_obs = observe(args.base_url, session)
        print(f"initial position={initial_obs['player']['position']}")

        block_1 = find_entity(initial_obs, "Block_1")
        assert_true(block_1 is not None, "Block_1 was not present in observation")
        print(f"initial Block_1 position={block_1['position']}")

        # Move toward Block_1's actual position
        move_target = block_1["position"]
        print(f"moving toward Block_1 at {move_target}")
        send_input(args.base_url, session, "MoveTo", {"position": move_target})
        moved_obs = wait_for_position(
            args.base_url,
            session,
            move_target,
            args.move_timeout,
            args.poll_interval,
        )
        print(f"arrived near pile position={moved_obs['player']['position']}")

        # Use actual Block_1 position for pickup instead of hardcoded target
        pre_pickup_obs = observe(args.base_url, session)
        block_1_now = find_entity(pre_pickup_obs, "Block_1")
        assert_true(block_1_now is not None, "Block_1 missing before pickup")
        pickup_pos = block_1_now["position"]
        print(f"Block_1 current position={pickup_pos}, picking up there")
        send_input(args.base_url, session, "Pickup", {"position": pickup_pos})
        time.sleep(args.poll_interval)
        pickup_obs = observe(args.base_url, session)
        pickup_attrs = pickup_obs["player"]["attributes"]
        pickup_block = find_entity(pickup_obs, "Block_1")
        assert_true(pickup_block is not None, "Block_1 missing after pickup")
        assert_true(pickup_attrs.get("HoldingBlock") is True, "Expected HoldingBlock=True after pickup")
        assert_true(
            pickup_attrs.get("HeldBlockColor") == "Sand",
            f"Expected HeldBlockColor=Sand after pickup, got {pickup_attrs.get('HeldBlockColor')!r}",
        )
        assert_true(
            pickup_attrs.get("LastBlockAction") in {"PickupSuccess", "PickupFailed:AlreadyHolding"},
            f"Unexpected LastBlockAction after pickup: {pickup_attrs.get('LastBlockAction')!r}",
        )
        assert_true(
            distance(pickup_block["position"], pickup_obs["player"]["position"]) > 2.5,
            "Expected held block to be lifted away from its original ground position",
        )
        print(f"pickup attrs={pickup_attrs}")
        print(f"Block_1 while held={pickup_block['position']}")

        send_input(args.base_url, session, "Place", {"position": args.place_target})
        time.sleep(args.poll_interval)
        place_obs = observe(args.base_url, session)
        place_attrs = place_obs["player"]["attributes"]
        placed_block = find_entity(place_obs, "Block_1")
        assert_true(placed_block is not None, "Block_1 missing after place")
        assert_true(place_attrs.get("HoldingBlock") is False, "Expected empty hands after place")
        assert_true(
            place_attrs.get("LastBlockAction") == "PlaceSuccess",
            f"Expected PlaceSuccess, got {place_attrs.get('LastBlockAction')!r}",
        )
        print(f"place attrs={place_attrs}")
        print(f"Block_1 after place={placed_block['position']}")

        # Wait and verify block doesn't fall through the ground
        print("waiting 3s to check block doesn't fall through ground...")
        time.sleep(3.0)
        settled_obs = observe(args.base_url, session)
        settled_block = find_entity(settled_obs, "Block_1")
        assert_true(settled_block is not None, "Block_1 missing after settling")
        settled_y = settled_block["position"][1]
        ground_top_y = 0.0  # ground is at y=-0.5 with height 1, so top is y=0
        min_y = ground_top_y - 0.5  # small tolerance for resting on surface
        assert_true(
            settled_y >= min_y,
            f"Block fell through ground! Block_1 y={settled_y} (expected >= {min_y})",
        )
        print(f"Block_1 settled position={settled_block['position']} (y={settled_y})")
        print("PASS")
        return 0
    except urllib.error.URLError as exc:
        print(
            f"Failed to reach {args.base_url}. Start the world first with: clawblox run worlds/mesa-world",
            file=sys.stderr,
        )
        print(str(exc), file=sys.stderr)
        return 2
    except AssertionError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
