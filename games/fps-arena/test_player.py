#!/usr/bin/env python3
"""Simple FPS Arena test agent.

Supports two backend modes:
- Local CLI server (`clawblox run`): session-based auth (`X-Session`)
- Hosted/API server (`/api/v1`): API-key auth (`Authorization: Bearer ...`)

Examples:
  # Local clawblox run server
  python test_player.py --api-base http://localhost:8080

  # Hosted/api server
  python test_player.py --api-base http://localhost:8080/api/v1 --game-id <uuid>
"""

from __future__ import annotations

import argparse
import math
import sys
import time
from dataclasses import dataclass
from typing import Any

import requests


@dataclass
class Session:
    mode: str  # "local" | "platform"
    api_base: str
    game_id: str | None
    headers: dict[str, str]


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Simple test player for FPS Arena")
    p.add_argument(
        "--api-base",
        default="http://localhost:8080",
        help="Base URL (local: http://localhost:8080, hosted: .../api/v1)",
    )
    p.add_argument("--game-id", default=None, help="Required for /api/v1 mode")
    p.add_argument("--name", default="fps-arena-test", help="Agent name")
    p.add_argument("--api-key", default=None, help="Existing API key for /api/v1 mode")
    p.add_argument("--duration", type=float, default=45.0, help="Run duration in seconds")
    p.add_argument("--tick", type=float, default=0.35, help="Loop interval in seconds")
    return p.parse_args()


def normalize_base(url: str) -> str:
    return url.rstrip("/")


def detect_mode(api_base: str) -> str:
    return "platform" if api_base.endswith("/api/v1") else "local"


def register_agent(api_base: str, name: str) -> str:
    payload = {"name": name, "description": "Simple FPS Arena smoke test bot"}
    r = requests.post(f"{api_base}/agents/register", json=payload, timeout=10)
    r.raise_for_status()
    body = r.json()
    key = body.get("api_key")
    if not isinstance(key, str) or not key:
        agent = body.get("agent")
        if isinstance(agent, dict):
            key = agent.get("api_key")
    if not isinstance(key, str) or not key:
        raise RuntimeError("register response missing api_key")
    agent_id = body.get("agent_id")
    if agent_id is None and isinstance(body.get("agent"), dict):
        agent_id = body["agent"].get("id")
    print(f"[register] agent_id={agent_id} name={name}")
    return key


def create_platform_session(args: argparse.Namespace, api_base: str) -> Session:
    if not args.game_id:
        raise RuntimeError("--game-id is required when --api-base ends with /api/v1")

    api_key = args.api_key or register_agent(api_base, args.name)
    headers = {"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"}

    r = requests.post(f"{api_base}/games/{args.game_id}/join", headers=headers, timeout=10)
    r.raise_for_status()
    print("[join] joined game (/api/v1 mode)")

    return Session(mode="platform", api_base=api_base, game_id=args.game_id, headers=headers)


def create_local_session(args: argparse.Namespace, api_base: str) -> Session:
    # `clawblox run` starts a daemon and may take a moment before endpoints respond.
    deadline = time.time() + 12.0
    last_err: str | None = None
    while time.time() < deadline:
        try:
            ping = requests.get(f"{api_base}/skill.md", timeout=2)
            if ping.status_code in (200, 404):
                break
        except Exception as exc:  # noqa: BLE001
            last_err = str(exc)
        time.sleep(0.25)
    else:
        raise RuntimeError(f"local server not ready at {api_base}: {last_err or 'timeout'}")

    r = requests.post(f"{api_base}/join", params={"name": args.name}, timeout=10)
    r.raise_for_status()
    body = r.json()

    token = body.get("session")
    if not isinstance(token, str) or not token:
        raise RuntimeError("local /join response missing session token")

    headers = {"X-Session": token, "Content-Type": "application/json"}
    print(f"[join] joined local game session={token[:8]}...")

    return Session(mode="local", api_base=api_base, game_id=args.game_id, headers=headers)


def leave_game(sess: Session) -> None:
    try:
        if sess.mode == "platform":
            assert sess.game_id is not None
            requests.post(f"{sess.api_base}/games/{sess.game_id}/leave", headers=sess.headers, timeout=5)
            print("[leave] left game")
        else:
            # local server has no leave endpoint; dropping session is enough
            print("[leave] local session ended")
    except Exception as exc:  # noqa: BLE001
        print(f"[leave] warning: {exc}")


def observe(sess: Session) -> dict[str, Any]:
    if sess.mode == "platform":
        assert sess.game_id is not None
        r = requests.get(f"{sess.api_base}/games/{sess.game_id}/observe", headers=sess.headers, timeout=10)
    else:
        r = requests.get(f"{sess.api_base}/observe", headers=sess.headers, timeout=10)
    r.raise_for_status()
    return r.json()


def send_input(sess: Session, input_type: str, data: dict[str, Any]) -> None:
    payload = {"type": input_type, "data": data}
    if sess.mode == "platform":
        assert sess.game_id is not None
        r = requests.post(f"{sess.api_base}/games/{sess.game_id}/input", headers=sess.headers, json=payload, timeout=10)
    else:
        r = requests.post(f"{sess.api_base}/input", headers=sess.headers, json=payload, timeout=10)
    r.raise_for_status()


def choose_move_target(t: float, radius: float = 32.0) -> list[float]:
    x = math.cos(t * 0.55) * radius
    z = math.sin(t * 0.55) * radius
    return [x, 2.0, z]


def choose_fire_target(obs: dict[str, Any], self_id: str | None) -> list[float] | None:
    others = obs.get("other_players") or []
    if not others:
        return None

    me = obs.get("player") or {}
    my_pos = me.get("position") or [0.0, 0.0, 0.0]

    best = None
    best_dist = float("inf")
    for p in others:
        pid = p.get("id")
        if self_id is not None and pid == self_id:
            continue
        pos = p.get("position")
        if not isinstance(pos, list) or len(pos) != 3:
            continue
        dx = float(pos[0]) - float(my_pos[0])
        dz = float(pos[2]) - float(my_pos[2])
        d = dx * dx + dz * dz
        if d < best_dist:
            best_dist = d
            best = [float(pos[0]), float(pos[1]) + 1.2, float(pos[2])]
    return best


def main() -> int:
    args = parse_args()
    api_base = normalize_base(args.api_base)
    mode = detect_mode(api_base)
    print(f"[mode] {mode} ({api_base})")

    try:
        if mode == "platform":
            sess = create_platform_session(args, api_base)
        else:
            sess = create_local_session(args, api_base)

        start = time.time()
        last_fire = 0.0
        self_id: str | None = None

        while time.time() - start < args.duration:
            now = time.time()
            obs = observe(sess)

            player = obs.get("player") or {}
            if self_id is None:
                pid = player.get("id")
                if isinstance(pid, str):
                    self_id = pid

            pos = player.get("position") or [0.0, 0.0, 0.0]
            hp = player.get("health")
            attrs = player.get("attributes") or {}
            kills = attrs.get("Kills")
            deaths = attrs.get("Deaths")
            score = attrs.get("Score")
            tick = obs.get("tick")

            print(
                f"[obs] tick={tick} hp={hp} pos=({pos[0]:.1f},{pos[1]:.1f},{pos[2]:.1f}) "
                f"kills={kills} deaths={deaths} score={score}"
            )

            move_target = choose_move_target(now - start)
            send_input(sess, "MoveTo", {"position": move_target})

            if now - last_fire >= 0.25:
                fire_target = choose_fire_target(obs, self_id)
                if fire_target is not None:
                    send_input(sess, "Fire", {"target": fire_target})
                    print(f"[fire] target=({fire_target[0]:.1f},{fire_target[1]:.1f},{fire_target[2]:.1f})")
                last_fire = now

            time.sleep(args.tick)

        print("[done] duration reached")
        return 0

    except requests.HTTPError as exc:
        detail = exc.response.text if exc.response is not None else str(exc)
        print(f"[fatal] HTTP error: {detail}")
        return 1
    except KeyboardInterrupt:
        print("[done] interrupted")
        return 0
    except Exception as exc:  # noqa: BLE001
        print(f"[fatal] {exc}")
        return 1
    finally:
        try:
            leave_game(sess)  # type: ignore[name-defined]
        except Exception:
            pass


if __name__ == "__main__":
    sys.exit(main())
