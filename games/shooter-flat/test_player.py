#!/usr/bin/env python3
"""Minimal test player: join, fire in one fixed direction, and validate shot counter."""

from __future__ import annotations

import argparse
import pathlib
import time
from typing import Any

import requests

API_BASE = "http://localhost:8080/api/v1"
DEFAULT_NAME = "ShooterFlatBot"
DEFAULT_TIMEOUT = 12.0
FIRE_TARGET = [0.0, 6.0, 80.0]
FIRE_INTERVAL = 0.45


class TestFailure(RuntimeError):
    pass


def read_local_game_id() -> str | None:
    game_id_file = pathlib.Path(__file__).resolve().parent / ".clawblox" / "game_id"
    if not game_id_file.exists():
        return None
    return game_id_file.read_text(encoding="utf-8").strip() or None


def ensure_api_key(name: str) -> str:
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": name, "description": "minimal shooter-flat test player"},
        timeout=10,
    )
    if resp.status_code not in (200, 201, 409):
        raise TestFailure(f"agent register failed: {resp.status_code} {resp.text}")
    if resp.status_code == 409:
        raise TestFailure(
            "agent name already exists; use --name <new-name> or pass --api-key explicitly"
        )
    data = resp.json()
    api_key = data.get("api_key")
    if not api_key and isinstance(data.get("agent"), dict):
        api_key = data["agent"].get("api_key")
    if not api_key:
        raise TestFailure(f"missing api_key in register response: {data}")
    return api_key


def observe(game_id: str, headers: dict[str, str]) -> dict[str, Any]:
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers, timeout=10)
    if resp.status_code != 200:
        raise TestFailure(f"observe failed: {resp.status_code} {resp.text}")
    return resp.json()


def send_input(game_id: str, headers: dict[str, str], input_type: str, data: dict[str, Any]) -> None:
    payload = {"type": input_type, "data": data}
    resp = requests.post(f"{API_BASE}/games/{game_id}/input", headers=headers, json=payload, timeout=10)
    if resp.status_code != 200:
        raise TestFailure(f"input {input_type} failed: {resp.status_code} {resp.text}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Minimal shooter-flat test player.")
    parser.add_argument("--name", default=DEFAULT_NAME)
    parser.add_argument("--api-key", default=None)
    parser.add_argument("--game-id", default=None)
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    args = parser.parse_args()

    game_id = args.game_id or read_local_game_id()
    if not game_id:
        raise TestFailure("missing game id; pass --game-id or deploy once to create .clawblox/game_id")

    api_key = args.api_key or ensure_api_key(args.name)
    headers = {"Authorization": f"Bearer {api_key}"}

    join_resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers, timeout=10)
    if join_resp.status_code != 200:
        raise TestFailure(f"join failed: {join_resp.status_code} {join_resp.text}")

    print(f"Joined game {game_id}. Firing repeatedly toward +Z...")

    sent = 0
    last_fire = 0.0
    start = time.time()
    observed_shots = 0

    while time.time() - start < args.timeout:
        now = time.time()
        if now - last_fire >= FIRE_INTERVAL:
            send_input(game_id, headers, "Fire", {"target": FIRE_TARGET, "weapon": "Rifle"})
            sent += 1
            last_fire = now
            print(f"fire#{sent} target={FIRE_TARGET}")

        obs = observe(game_id, headers)
        attrs = (obs.get("player") or {}).get("attributes") or {}
        observed_shots = int(attrs.get("ShotsFired") or 0)
        print(f"observed ShotsFired={observed_shots}")
        time.sleep(0.2)

    if sent < 5:
        raise TestFailure(f"expected >=5 fire inputs, got {sent}")
    if observed_shots <= 0:
        raise TestFailure("shots were sent but ShotsFired attribute never increased")

    print("PASS: one-direction shooting loop validated.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except TestFailure as exc:
        print(f"FAIL: {exc}")
        raise SystemExit(1)
