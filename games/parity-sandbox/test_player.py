#!/usr/bin/env python3
"""
Parity Sandbox shooter gameplay test agent.

What it validates:
- Can join the deployed Parity Sandbox game
- MoveTo input works while the round is active
- Shooter inputs work (SetWeapon + Fire)
- Combat progression occurs (shots, damage, kills)

This script is strict: it raises on any critical failure.
"""

import argparse
import json
import os
import random
import time
from pathlib import Path

import requests

# Load .env if present
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
KEYS_CACHE = Path("/tmp/clawblox_parity_sandbox_keys.json")
DEFAULT_GAME_NAME = "Parity Sandbox"
DEFAULT_TIMEOUT_SECS = 45.0
MOVE_INTERVAL_SECS = 0.30
OBS_INTERVAL_SECS = 0.20
MOVEMENT_MIN_TOTAL_XZ = 12.0
MOVEMENT_MIN_STEP_XZ = 0.75

# Keep movement inside the current map bounds around spawn/cover.
ROAM_MIN_X = -28.0
ROAM_MAX_X = 28.0
ROAM_MIN_Z = -24.0
ROAM_MAX_Z = 24.0
ROAM_Y = 6.0
ENEMY_SPAWN_POINTS = [
    [-46.0, 4.0, -32.0],
    [46.0, 4.0, -32.0],
    [-46.0, 4.0, 32.0],
    [46.0, 4.0, 32.0],
]


class TestFailure(RuntimeError):
    pass


def distance_xz(a: list[float], b: list[float]) -> float:
    dx = float(a[0]) - float(b[0])
    dz = float(a[2]) - float(b[2])
    return (dx * dx + dz * dz) ** 0.5


def load_cached_keys() -> list[str]:
    if KEYS_CACHE.exists():
        try:
            return json.loads(KEYS_CACHE.read_text()).get("keys", [])
        except Exception:
            return []
    return []


def save_cached_keys(keys: list[str]) -> None:
    KEYS_CACHE.write_text(json.dumps({"keys": keys}, indent=2))


def register_agent(name: str) -> str | None:
    resp = requests.post(
        f"{API_BASE}/agents/register",
        json={"name": name, "description": "Parity sandbox shooter test agent"},
        timeout=10,
    )

    if resp.status_code == 200:
        return resp.json()["agent"]["api_key"]
    if resp.status_code == 409:
        return None

    raise TestFailure(f"Registration failed: {resp.status_code} {resp.text}")


def ensure_api_key(preferred_name: str | None) -> str:
    env_key = os.getenv("CLAWBLOX_API_KEY")
    if env_key:
        return env_key

    cached = load_cached_keys()
    if cached:
        return cached[0]

    names_to_try: list[str] = []
    if preferred_name:
        names_to_try.append(preferred_name)
    names_to_try.extend([f"parity_sandbox_{random.randint(1000, 9999)}" for _ in range(8)])

    for name in names_to_try:
        key = register_agent(name)
        if key:
            save_cached_keys([key])
            print(f"Registered agent: {name}")
            return key

    raise TestFailure("Failed to obtain API key (registration/cached/env all unavailable).")


def resolve_game_id(headers: dict[str, str], explicit_game_id: str | None) -> str:
    if explicit_game_id:
        return explicit_game_id

    local_id_path = Path(__file__).parent / ".clawblox" / "game_id"
    if local_id_path.exists():
        text = local_id_path.read_text().strip()
        if text:
            return text

    resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=10)
    if resp.status_code != 200:
        raise TestFailure(f"Failed to list games: {resp.status_code} {resp.text}")

    games = resp.json().get("games", [])
    for game in games:
        if game.get("name") == DEFAULT_GAME_NAME:
            return game["id"]

    raise TestFailure(
        "Could not resolve game id. Pass --game-id or deploy first via:\n"
        "  ./scripts/deploy_local_game.sh --game games/parity-sandbox"
    )


def observe(game_id: str, headers: dict[str, str]) -> dict:
    resp = requests.get(f"{API_BASE}/games/{game_id}/observe", headers=headers, timeout=5)
    if resp.status_code != 200:
        raise TestFailure(f"Observe failed: {resp.status_code} {resp.text}")
    return resp.json()


def send_input(game_id: str, headers: dict[str, str], input_type: str, data: dict | None = None) -> None:
    payload: dict = {"type": input_type}
    if data is not None:
        payload["data"] = data

    resp = requests.post(
        f"{API_BASE}/games/{game_id}/input",
        headers=headers,
        json=payload,
        timeout=5,
    )
    if resp.status_code != 200:
        raise TestFailure(f"Input {input_type} failed: {resp.status_code} {resp.text}")


def leave_all_games(headers: dict[str, str]) -> None:
    resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
    if resp.status_code != 200:
        raise TestFailure(f"Failed to list games before join: {resp.status_code} {resp.text}")

    for game in resp.json().get("games", []):
        leave_resp = requests.post(f"{API_BASE}/games/{game['id']}/leave", headers=headers, timeout=5)
        if leave_resp.status_code >= 500:
            raise TestFailure(
                f"Leave failed for game {game['id']}: {leave_resp.status_code} {leave_resp.text}"
            )


def extract_round_state(obs: dict) -> tuple[str, bool, str, float]:
    for entity in obs.get("world", {}).get("entities", []):
        if entity.get("name") == "RoundMarker":
            attrs = entity.get("attributes") or {}
            phase = str(attrs.get("Phase", ""))
            is_finished = bool(attrs.get("IsFinished", False))
            winner_name = str(attrs.get("WinnerName", ""))
            winner_user_id = float(attrs.get("WinnerUserId", 0) or 0)
            return phase, is_finished, winner_name, winner_user_id
    return "", False, "", 0.0


def extract_player_combat_stats(obs: dict) -> tuple[float, float, float, str]:
    player = obs.get("player") or {}
    attrs = player.get("attributes") or {}
    shots = float(attrs.get("ShotsFired", 0) or 0)
    damage = float(attrs.get("DamageDealt", 0) or 0)
    kills = float(attrs.get("Kills", 0) or 0)
    weapon = str(attrs.get("CurrentWeapon", ""))
    return shots, damage, kills, weapon


def count_alive_enemies(obs: dict) -> int:
    alive = 0
    for entity in obs.get("world", {}).get("entities", []):
        attrs = entity.get("attributes") or {}
        name = str(entity.get("name", ""))
        enemy_type = attrs.get("EnemyType")
        if enemy_type == "Zombie" or name.startswith("ZombieRoot_") or name.startswith("Zombie_"):
            if attrs.get("Alive", True):
                alive += 1
    return alive


def find_nearest_enemy_target(obs: dict, from_pos: list[float]) -> list[float] | None:
    nearest = None
    nearest_d2 = None
    for entity in obs.get("world", {}).get("entities", []):
        name = str(entity.get("name", ""))
        attrs = entity.get("attributes") or {}
        if not (
            name.startswith("Zombie_")
            or name.startswith("ZombieRoot_")
            or attrs.get("EnemyType") == "Zombie"
        ):
            continue

        if attrs.get("Alive") is False:
            continue

        pos = entity.get("position")
        if not isinstance(pos, list) or len(pos) < 3:
            continue

        dx = float(pos[0]) - float(from_pos[0])
        dy = float(pos[1]) - float(from_pos[1])
        dz = float(pos[2]) - float(from_pos[2])
        d2 = dx * dx + dy * dy + dz * dz
        if nearest is None or (nearest_d2 is not None and d2 < nearest_d2):
            nearest = [float(pos[0]), float(pos[1]), float(pos[2])]
            nearest_d2 = d2

    return nearest


def random_roam_target() -> list[float]:
    return [
        random.uniform(ROAM_MIN_X, ROAM_MAX_X),
        ROAM_Y,
        random.uniform(ROAM_MIN_Z, ROAM_MAX_Z),
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description="Parity Sandbox shooter test player")
    parser.add_argument("--game-id", help="Game UUID (optional; auto-resolved if omitted)")
    parser.add_argument("--name", help="Preferred agent name for registration")
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_SECS,
        help=f"Seconds to run combat validation (default: {DEFAULT_TIMEOUT_SECS})",
    )
    args = parser.parse_args()

    api_key = ensure_api_key(args.name)
    headers = {"Authorization": f"Bearer {api_key}"}
    game_id = resolve_game_id(headers, args.game_id)

    print(f"Using game: {game_id}")

    leave_all_games(headers)

    join_resp = requests.post(f"{API_BASE}/games/{game_id}/join", headers=headers, timeout=10)
    if join_resp.status_code != 200:
        raise TestFailure(f"Join failed: {join_resp.status_code} {join_resp.text}")

    print("Joined game. Equipping rifle and playing shooter loop...")

    start = time.time()
    last_move_at = 0.0
    last_obs_log_at = 0.0
    next_spawn_idx = 0
    move_commands_sent = 0
    fire_commands_sent = 0
    total_moved_xz = 0.0
    max_step_xz = 0.0

    saw_active_phase = False
    saw_enemy_target = False
    saw_shot = False
    saw_damage = False
    saw_kill = False
    saw_observed_movement = False
    rejoin_attempts = 0
    last_obs_pos: list[float] | None = None
    last_seen_shots = 0.0
    last_seen_damage = 0.0
    last_seen_kills = 0.0

    try:
        # Keep weapon deterministic.
        send_input(game_id, headers, "SetWeapon", {"weapon": "Rifle"})

        while time.time() - start < args.timeout:
            obs = observe(game_id, headers)
            player = obs.get("player") or {}
            player_pos = player.get("position") or [-8.0, 6.0, 0.0]

            phase, is_finished, winner_name, winner_user_id = extract_round_state(obs)
            shots, damage, kills, weapon = extract_player_combat_stats(obs)

            if phase == "Active":
                saw_active_phase = True

            enemy_target = find_nearest_enemy_target(obs, player_pos)
            if enemy_target is not None:
                saw_enemy_target = True
                # Aim roughly at torso/head level.
                fire_target = [enemy_target[0], enemy_target[1] + 1.0, enemy_target[2]]
                fire_commands_sent += 1
                if fire_commands_sent % 6 == 1:
                    print(
                        f"ACTION fire#{fire_commands_sent} mode=target "
                        f"target=({fire_target[0]:.1f},{fire_target[1]:.1f},{fire_target[2]:.1f})"
                    )
                send_input(
                    game_id,
                    headers,
                    "Fire",
                    {"target": fire_target, "weapon": weapon or "Rifle"},
                )
            elif phase == "Active":
                fallback_target = ENEMY_SPAWN_POINTS[next_spawn_idx % len(ENEMY_SPAWN_POINTS)]
                next_spawn_idx += 1
                fire_commands_sent += 1
                if fire_commands_sent % 6 == 1:
                    print(
                        f"ACTION fire#{fire_commands_sent} mode=fallback "
                        f"target=({fallback_target[0]:.1f},{fallback_target[1] + 1.0:.1f},{fallback_target[2]:.1f})"
                    )
                send_input(
                    game_id,
                    headers,
                    "Fire",
                    {"target": [fallback_target[0], fallback_target[1] + 1.0, fallback_target[2]], "weapon": weapon or "Rifle"},
                )

            now = time.time()
            if now - last_move_at >= MOVE_INTERVAL_SECS:
                if phase == "Active":
                    lane_target = ENEMY_SPAWN_POINTS[next_spawn_idx % len(ENEMY_SPAWN_POINTS)]
                    move_target = [lane_target[0] * 0.6, ROAM_Y, lane_target[2] * 0.6]
                    move_commands_sent += 1
                    print(
                        f"ACTION move#{move_commands_sent} mode=lane "
                        f"target=({move_target[0]:.1f},{move_target[1]:.1f},{move_target[2]:.1f})"
                    )
                    send_input(game_id, headers, "MoveTo", {"position": move_target})
                else:
                    roam_target = random_roam_target()
                    move_commands_sent += 1
                    print(
                        f"ACTION move#{move_commands_sent} mode=roam "
                        f"target=({roam_target[0]:.1f},{roam_target[1]:.1f},{roam_target[2]:.1f})"
                    )
                    send_input(game_id, headers, "MoveTo", {"position": roam_target})
                last_move_at = now

            if last_obs_pos is not None:
                step_xz = distance_xz(player_pos, last_obs_pos)
                total_moved_xz = total_moved_xz + step_xz
                if step_xz > max_step_xz:
                    max_step_xz = step_xz
                if step_xz >= MOVEMENT_MIN_STEP_XZ:
                    saw_observed_movement = True
            last_obs_pos = [float(player_pos[0]), float(player_pos[1]), float(player_pos[2])]

            if shots > 0:
                saw_shot = True
            if damage > 0:
                saw_damage = True
            if kills > 0:
                saw_kill = True

            if shots > last_seen_shots:
                print(
                    f"EVENT shots +{shots - last_seen_shots:.0f} => {shots:.0f}"
                )
                last_seen_shots = shots
            if damage > last_seen_damage:
                print(
                    f"EVENT damage +{damage - last_seen_damage:.0f} => {damage:.0f}"
                )
                last_seen_damage = damage
            if kills > last_seen_kills:
                print(
                    f"EVENT kills +{kills - last_seen_kills:.0f} => {kills:.0f}"
                )
                last_seen_kills = kills

            # Treat this as a successful play test as soon as combat is clearly happening.
            if (
                saw_active_phase
                and saw_shot
                and saw_damage
                and saw_kill
                and saw_observed_movement
                and total_moved_xz >= MOVEMENT_MIN_TOTAL_XZ
            ):
                print("PASS: player actively played shooter loop with combat progression.")
                return 0

            if now - last_obs_log_at >= 1.0:
                enemy_count = count_alive_enemies(obs)
                print(
                    f"phase={phase or '?'} shots={shots:.0f} damage={damage:.0f} "
                    f"kills={kills:.0f} alive_enemies={enemy_count} target={'yes' if enemy_target else 'no'} "
                    f"move_cmds={move_commands_sent} fire_cmds={fire_commands_sent} "
                    f"move_xz_total={total_moved_xz:.1f} move_xz_max={max_step_xz:.2f}"
                )
                last_obs_log_at = now

            if is_finished:
                print(f"Round finished. winner={winner_name!r} winner_user_id={winner_user_id!r}")
                if not saw_active_phase and rejoin_attempts < 2:
                    rejoin_attempts += 1
                    print("Round already finished on join; rejoining for a fresh round...")
                    requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers, timeout=5)
                    join_resp = requests.post(
                        f"{API_BASE}/games/{game_id}/join",
                        headers=headers,
                        timeout=10,
                    )
                    if join_resp.status_code != 200:
                        raise TestFailure(
                            f"Rejoin failed: {join_resp.status_code} {join_resp.text}"
                        )
                    send_input(game_id, headers, "SetWeapon", {"weapon": "Rifle"})
                    start = time.time()
                    last_move_at = 0.0
                    last_obs_log_at = 0.0
                    continue
                break

            time.sleep(OBS_INTERVAL_SECS)

        if not saw_active_phase:
            raise TestFailure("Did not observe Active combat phase.")
        if not saw_enemy_target:
            raise TestFailure("Never observed a live zombie target in world entities.")
        if not saw_shot:
            raise TestFailure("No shot was recorded (ShotsFired stayed at 0).")
        if not saw_damage:
            raise TestFailure("No damage was recorded (DamageDealt stayed at 0).")
        if not saw_kill:
            raise TestFailure("No kill was recorded (Kills stayed at 0).")
        if move_commands_sent == 0:
            raise TestFailure("No MoveTo commands were sent.")
        if not saw_observed_movement:
            raise TestFailure("Player position never showed a meaningful movement step.")
        if total_moved_xz < MOVEMENT_MIN_TOTAL_XZ:
            raise TestFailure(
                f"Player moved too little (total xz={total_moved_xz:.1f} < {MOVEMENT_MIN_TOTAL_XZ:.1f})."
            )

        print("PASS: player actively played shooter loop with combat progression.")
        return 0
    finally:
        leave_resp = requests.post(f"{API_BASE}/games/{game_id}/leave", headers=headers, timeout=5)
        if leave_resp.status_code >= 500:
            raise TestFailure(f"Leave failed: {leave_resp.status_code} {leave_resp.text}")


if __name__ == "__main__":
    raise SystemExit(main())
