#!/usr/bin/env python3
"""
Test agent focused on movement. Explores the arena by moving to random waypoints.
"""

import argparse
import json
import math
import os
from pathlib import Path
import random
import statistics
import sys
import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field

import requests

# Load .env file if present (project root is ../../)
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE_LOCAL = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
API_BASE_PROD = os.getenv("CLAWBLOX_API_URL_PROD", "https://clawblox.com/api/v1")
API_BASE = API_BASE_LOCAL  # Default, can be overridden by --prod flag

KEYS_CACHE = Path("/tmp/clawblox_agent_keys.json")
KEYS_CACHE_PROD = Path("/tmp/clawblox_agent_keys_prod.json")


@dataclass
class LatencyStats:
    """Thread-safe latency statistics collector"""
    lock: threading.Lock = field(default_factory=threading.Lock)
    latencies: dict = field(default_factory=lambda: defaultdict(list))
    errors: dict = field(default_factory=lambda: defaultdict(int))
    timeouts: dict = field(default_factory=lambda: defaultdict(int))

    def record(self, endpoint: str, latency_ms: float):
        with self.lock:
            self.latencies[endpoint].append(latency_ms)

    def record_error(self, endpoint: str):
        with self.lock:
            self.errors[endpoint] += 1

    def record_timeout(self, endpoint: str):
        with self.lock:
            self.timeouts[endpoint] += 1

    def get_summary(self) -> dict:
        with self.lock:
            summary = {}
            for endpoint, lats in self.latencies.items():
                if not lats:
                    continue
                sorted_lats = sorted(lats)
                n = len(sorted_lats)
                summary[endpoint] = {
                    "count": n,
                    "min": sorted_lats[0],
                    "max": sorted_lats[-1],
                    "mean": statistics.mean(sorted_lats),
                    "p50": sorted_lats[n // 2],
                    "p95": sorted_lats[int(n * 0.95)] if n >= 20 else sorted_lats[-1],
                    "p99": sorted_lats[int(n * 0.99)] if n >= 100 else sorted_lats[-1],
                    "errors": self.errors.get(endpoint, 0),
                    "timeouts": self.timeouts.get(endpoint, 0),
                }
            return summary

    def print_summary(self):
        summary = self.get_summary()
        if not summary:
            print("No latency data collected")
            return

        print("\n" + "=" * 70)
        print("LATENCY SUMMARY (milliseconds)")
        print("=" * 70)
        print(f"{'Endpoint':<15} {'Count':>8} {'Min':>8} {'Mean':>8} {'P50':>8} {'P95':>8} {'P99':>8} {'Max':>8} {'Err':>5} {'T/O':>5}")
        print("-" * 70)

        for endpoint in sorted(summary.keys()):
            s = summary[endpoint]
            print(f"{endpoint:<15} {s['count']:>8} {s['min']:>8.1f} {s['mean']:>8.1f} {s['p50']:>8.1f} {s['p95']:>8.1f} {s['p99']:>8.1f} {s['max']:>8.1f} {s['errors']:>5} {s['timeouts']:>5}")

        # Overall stats
        all_lats = []
        total_errors = 0
        total_timeouts = 0
        for endpoint, s in summary.items():
            all_lats.extend(self.latencies[endpoint])
            total_errors += s['errors']
            total_timeouts += s['timeouts']

        if all_lats:
            sorted_all = sorted(all_lats)
            n = len(sorted_all)
            print("-" * 70)
            print(f"{'TOTAL':<15} {n:>8} {sorted_all[0]:>8.1f} {statistics.mean(sorted_all):>8.1f} {sorted_all[n//2]:>8.1f} {sorted_all[int(n*0.95)]:>8.1f} {sorted_all[int(n*0.99)]:>8.1f} {sorted_all[-1]:>8.1f} {total_errors:>5} {total_timeouts:>5}")
        print("=" * 70)


# Global latency tracker
LATENCY_STATS = LatencyStats()


def timed_request(method: str, url: str, endpoint_name: str, **kwargs) -> requests.Response | None:
    """Make a request and record its latency"""
    start = time.perf_counter()
    try:
        if method == "GET":
            resp = requests.get(url, **kwargs)
        else:
            resp = requests.post(url, **kwargs)
        latency_ms = (time.perf_counter() - start) * 1000
        LATENCY_STATS.record(endpoint_name, latency_ms)
        return resp
    except requests.exceptions.Timeout:
        LATENCY_STATS.record_timeout(endpoint_name)
        return None
    except requests.exceptions.RequestException:
        LATENCY_STATS.record_error(endpoint_name)
        return None

# Arena bounds (200x200, stay inside walls)
ARENA_MIN = -80
ARENA_MAX = 80


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
            json={"name": name, "description": "Test agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def get_api_keys(num_needed: int, is_prod: bool = False) -> list[str]:
    """Get or register enough API keys for num_needed agents"""
    # Start with env key (use prod-specific key if in prod mode)
    keys = []
    env_key = os.getenv("CLAWBLOX_API_KEY_PROD" if is_prod else "CLAWBLOX_API_KEY")
    if env_key:
        keys.append(env_key)

    # Add cached keys
    for k in load_cached_keys():
        if k not in keys:
            keys.append(k)

    # Register more if needed
    while len(keys) < num_needed:
        name = f"agent_{random.randint(1000, 9999)}"
        print(f"Registering {name}...", flush=True)
        key = register_agent(name)
        if key:
            keys.append(key)
            print(f"  OK: {key[:20]}...", flush=True)

    # Cache all keys
    save_cached_keys(keys)
    return keys[:num_needed]


def distance_xz(a: list, b: list) -> float:
    return ((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** 0.5


def random_waypoint() -> list:
    return [random.uniform(ARENA_MIN, ARENA_MAX), 3.0, random.uniform(ARENA_MIN, ARENA_MAX)]


def unstuck_waypoint(pos: list, last_waypoint: list) -> list:
    """When stuck, pick a waypoint in the opposite direction from where we were heading"""
    # Direction we were trying to go
    dx = last_waypoint[0] - pos[0]
    dz = last_waypoint[2] - pos[2]

    # Go roughly opposite direction, with some randomness
    angle = random.uniform(-0.5, 0.5)  # radians of randomness
    import math
    cos_a, sin_a = math.cos(angle), math.sin(angle)

    # Rotate and reverse direction
    new_dx = -(dx * cos_a - dz * sin_a)
    new_dz = -(dx * sin_a + dz * cos_a)

    # Normalize and scale to a reasonable distance (20-40 units)
    dist = (new_dx**2 + new_dz**2) ** 0.5
    if dist > 0:
        scale = random.uniform(20, 40) / dist
        new_dx *= scale
        new_dz *= scale

    # Clamp to arena bounds
    new_x = max(ARENA_MIN, min(ARENA_MAX, pos[0] + new_dx))
    new_z = max(ARENA_MIN, min(ARENA_MAX, pos[2] + new_dz))

    return [new_x, 3.0, new_z]


def find_closest_enemy(pos: list, other_players: list) -> dict | None:
    """Find the closest visible enemy"""
    if not other_players:
        return None

    closest = None
    closest_dist = float('inf')

    for enemy in other_players:
        dist = distance_xz(pos, enemy["position"])
        if dist < closest_dist:
            closest_dist = dist
            closest = enemy

    return closest


def run_agent(agent_id: int, api_key: str, game_id: str, stop_event: threading.Event, cycle_delay: float):
    """Run a single agent"""
    prefix = f"[{agent_id}]"
    headers = {"Authorization": f"Bearer {api_key}"}

    # Leave any existing games
    try:
        resp = requests.get(f"{API_BASE}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{API_BASE}/games/{g['id']}/leave", headers=headers, timeout=5)
    except:
        pass

    # Join
    resp = timed_request("POST", f"{API_BASE}/games/{game_id}/join", "join", headers=headers, timeout=5)
    if resp is None or resp.status_code != 200:
        print(f"{prefix} Failed to join: {resp.text if resp else 'timeout'}", flush=True)
        return
    print(f"{prefix} Joined", flush=True)

    time.sleep(0.3)

    waypoint = random_waypoint()
    last_pos = None
    stuck_time = None
    arrivals = 0
    kills = 0
    last_status_time = 0

    try:
        while not stop_event.is_set():
            # Observe
            resp = timed_request("GET", f"{API_BASE}/games/{game_id}/observe", "observe", headers=headers, timeout=5)
            if resp is None or resp.status_code != 200:
                time.sleep(0.5)
                continue

            obs = resp.json()
            pos = obs["player"]["position"]
            other_players = obs.get("other_players", [])
            now = time.time()

            # Track kills from attributes
            player_kills = obs.get("player", {}).get("attributes", {}).get("Kills", 0)
            if player_kills > kills:
                print(f"{prefix} [KILL] total: {player_kills}", flush=True)
                kills = player_kills

            # Find closest enemy
            enemy = find_closest_enemy(pos, other_players)

            if enemy:
                enemy_pos = enemy["position"]
                enemy_dist = distance_xz(pos, enemy_pos)

                # Fire at enemy
                fire_payload = {"type": "Fire", "data": {"target": enemy_pos}}
                timed_request("POST", f"{API_BASE}/games/{game_id}/input", "input", headers=headers, json=fire_payload, timeout=5)

                # Move toward enemy
                waypoint = enemy_pos
                stuck_time = None

                if now - last_status_time >= 5.0:
                    print(f"{prefix} [COMBAT] enemy at dist {enemy_dist:.1f}", flush=True)
                    last_status_time = now
            else:
                # No enemy - explore
                dist = distance_xz(pos, waypoint)

                # Check arrival
                if dist < 5.0:
                    arrivals += 1
                    waypoint = random_waypoint()
                    stuck_time = None

                # Check stuck
                if last_pos:
                    moved = distance_xz(pos, last_pos)
                    if moved < 0.1:
                        if stuck_time is None:
                            stuck_time = time.time()
                        elif time.time() - stuck_time > 2.0:
                            waypoint = unstuck_waypoint(pos, waypoint)
                            stuck_time = None
                    else:
                        stuck_time = None

            # Send MoveTo
            move_payload = {"type": "MoveTo", "data": {"position": waypoint}}
            timed_request("POST", f"{API_BASE}/games/{game_id}/input", "input", headers=headers, json=move_payload, timeout=5)

            # Periodic status (every ~5 seconds)
            if now - last_status_time >= 5.0:
                health = obs.get("player", {}).get("health", 100)
                print(f"{prefix} pos=({pos[0]:.0f},{pos[2]:.0f}) hp={health} enemies={len(other_players)}", flush=True)
                last_status_time = now

            last_pos = pos
            time.sleep(cycle_delay)

    finally:
        timed_request("POST", f"{API_BASE}/games/{game_id}/leave", "leave", headers=headers, timeout=5)
        print(f"{prefix} Left (arrivals: {arrivals}, kills: {kills})", flush=True)


def print_live_stats():
    """Print a one-line summary of current latency stats"""
    summary = LATENCY_STATS.get_summary()
    if not summary:
        return

    parts = []
    for endpoint in ["observe", "input"]:
        if endpoint in summary:
            s = summary[endpoint]
            parts.append(f"{endpoint}: {s['mean']:.0f}ms (p99={s['p99']:.0f}ms, n={s['count']})")

    total_timeouts = sum(s.get('timeouts', 0) for s in summary.values())
    total_errors = sum(s.get('errors', 0) for s in summary.values())

    if total_timeouts or total_errors:
        parts.append(f"errors={total_errors} timeouts={total_timeouts}")

    if parts:
        print(f"[LATENCY] {' | '.join(parts)}", flush=True)


def main():
    global API_BASE, KEYS_CACHE

    parser = argparse.ArgumentParser(description="Test exploration agents")
    parser.add_argument("-n", "--num-agents", type=int, default=1, help="Number of agents")
    parser.add_argument("-d", "--duration", type=float, default=None, help="Run for N seconds")
    parser.add_argument("--stats-interval", type=float, default=10.0, help="Print latency stats every N seconds")
    parser.add_argument("--rate", type=float, default=1.0, help="Request cycles per second per agent (default: 1.0 for LLM-realistic pacing)")
    parser.add_argument("--prod", action="store_true", help="Run against production (clawblox.com)")
    args = parser.parse_args()

    # Switch to production if requested
    if args.prod:
        API_BASE = API_BASE_PROD
        KEYS_CACHE = KEYS_CACHE_PROD
        print("*** PRODUCTION MODE ***", flush=True)

    cycle_delay = 1.0 / args.rate if args.rate > 0 else 0.1

    print(f"API: {API_BASE}", flush=True)
    print(f"Rate: {args.rate} cycles/s per agent ({cycle_delay:.2f}s delay)", flush=True)

    # Get API keys
    api_keys = get_api_keys(args.num_agents, is_prod=args.prod)
    print(f"Got {len(api_keys)} API key(s)", flush=True)

    # Find game
    headers = {"Authorization": f"Bearer {api_keys[0]}"}
    resp = requests.get(f"{API_BASE}/games", headers=headers)
    resp.raise_for_status()
    games = resp.json().get("games", [])
    if not games:
        print("No games available")
        sys.exit(1)

    game_id = games[0]["id"]
    print(f"Game: {games[0]['name']}", flush=True)
    print("-" * 60, flush=True)

    # Start agents
    stop_event = threading.Event()
    threads = []

    for i in range(args.num_agents):
        t = threading.Thread(target=run_agent, args=(i, api_keys[i], game_id, stop_event, cycle_delay))
        t.daemon = True
        t.start()
        threads.append(t)
        time.sleep(0.2)  # Stagger joins

    # Run until duration or Ctrl+C
    try:
        start = time.time()
        last_stats = start
        while True:
            time.sleep(0.5)

            # Print periodic latency stats
            now = time.time()
            if now - last_stats >= args.stats_interval:
                print_live_stats()
                last_stats = now

            if args.duration and (now - start) >= args.duration:
                print(f"\nDuration {args.duration}s reached", flush=True)
                break
    except KeyboardInterrupt:
        print("\nStopping...", flush=True)
    finally:
        stop_event.set()
        for t in threads:
            t.join(timeout=2)

        # Print final latency summary
        LATENCY_STATS.print_summary()


if __name__ == "__main__":
    main()
