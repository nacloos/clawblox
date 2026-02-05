#!/usr/bin/env python3
"""
Test rate limiting on the API.
Expected: 10 req/sec per API key, burst of 20.
"""

import os
import time
import requests
from pathlib import Path

# Load .env
env_path = Path(__file__).parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
GAME_ID = "0a62727e-b45e-4175-be9f-1070244f8885"


def get_or_create_agent(api_base: str) -> str:
    """Get existing or create new agent, return API key."""
    # Try cached key first
    cache_file = Path("/tmp/rate_limit_test_key.txt")
    if cache_file.exists():
        key = cache_file.read_text().strip()
        resp = requests.get(f"{api_base}/agents/me", headers={"Authorization": f"Bearer {key}"}, timeout=5)
        if resp.status_code == 200:
            return key

    # Register new agent
    resp = requests.post(
        f"{api_base}/agents/register",
        json={"name": f"rate_limit_test_{int(time.time())}", "description": "Rate limit test"},
        timeout=10,
    )
    if resp.status_code != 200:
        raise Exception(f"Failed to register: {resp.text}")

    key = resp.json()["agent"]["api_key"]
    cache_file.write_text(key)
    return key


def test_burst(api_key: str, num_requests: int = 25):
    """Send burst of requests, count successes vs 429s."""
    headers = {"Authorization": f"Bearer {api_key}"}
    url = f"{API_BASE}/games/{GAME_ID}/observe"

    print(f"\n--- Burst Test: {num_requests} rapid requests ---")

    results = {"200": 0, "429": 0, "other": 0}
    rate_limit_headers = None

    start = time.time()
    for i in range(num_requests):
        resp = requests.get(url, headers=headers, timeout=5)

        if resp.status_code == 200:
            results["200"] += 1
        elif resp.status_code == 429:
            results["429"] += 1
            if rate_limit_headers is None:
                rate_limit_headers = {
                    k: v for k, v in resp.headers.items()
                    if k.lower().startswith("x-ratelimit") or k.lower() == "retry-after"
                }
        else:
            results["other"] += 1
            print(f"  Unexpected status: {resp.status_code}")

    elapsed = time.time() - start

    print(f"  Sent {num_requests} requests in {elapsed:.2f}s ({num_requests/elapsed:.1f} req/s)")
    print(f"  Results: {results['200']} OK, {results['429']} rate-limited, {results['other']} other")

    if rate_limit_headers:
        print(f"  Rate limit headers: {rate_limit_headers}")

    # With burst=20, first ~20 should succeed, rest should be 429
    if results["200"] >= 15 and results["429"] > 0:
        print("  PASS: Burst allowed, then rate-limited")
    elif results["429"] == 0:
        print("  WARN: No rate limiting observed (requests too slow?)")
    else:
        print(f"  INFO: {results['200']} succeeded before rate limit kicked in")


def test_sustained_rate(api_key: str, rate: float, duration: float = 3.0):
    """Test sustained request rate."""
    headers = {"Authorization": f"Bearer {api_key}"}
    url = f"{API_BASE}/games/{GAME_ID}/observe"

    interval = 1.0 / rate
    print(f"\n--- Sustained Rate Test: {rate} req/s for {duration}s ---")

    results = {"200": 0, "429": 0}
    start = time.time()

    while time.time() - start < duration:
        req_start = time.time()
        resp = requests.get(url, headers=headers, timeout=5)

        if resp.status_code == 200:
            results["200"] += 1
        elif resp.status_code == 429:
            results["429"] += 1

        # Sleep to maintain rate
        elapsed = time.time() - req_start
        sleep_time = max(0, interval - elapsed)
        time.sleep(sleep_time)

    total = results["200"] + results["429"]
    print(f"  Sent {total} requests")
    print(f"  Results: {results['200']} OK, {results['429']} rate-limited")

    if results["429"] == 0:
        print(f"  PASS: All requests succeeded at {rate} req/s")
    else:
        print(f"  FAIL: Got rate-limited at {rate} req/s")


def test_recovery(api_key: str):
    """Test that rate limit recovers after waiting."""
    headers = {"Authorization": f"Bearer {api_key}"}
    url = f"{API_BASE}/games/{GAME_ID}/observe"

    print("\n--- Recovery Test ---")

    # Exhaust burst
    print("  Exhausting burst...")
    for _ in range(25):
        requests.get(url, headers=headers, timeout=5)

    # Check we're rate limited
    resp = requests.get(url, headers=headers, timeout=5)
    if resp.status_code != 429:
        print("  WARN: Not rate-limited after burst (unexpected)")
        return

    print("  Rate-limited. Waiting 2 seconds...")
    time.sleep(2)

    # Should have recovered ~20 tokens
    successes = 0
    for _ in range(15):
        resp = requests.get(url, headers=headers, timeout=5)
        if resp.status_code == 200:
            successes += 1

    print(f"  After 2s wait: {successes}/15 requests succeeded")
    if successes >= 10:
        print("  PASS: Rate limit recovered")
    else:
        print("  FAIL: Rate limit did not recover as expected")


def main():
    print(f"API: {API_BASE}")
    print(f"Game: {GAME_ID}")
    print(f"Rate limit config: 10 req/s, burst 20")

    api_key = get_or_create_agent(API_BASE)
    print(f"Using API key: {api_key[:20]}...")

    # Join game first (needed for /observe)
    headers = {"Authorization": f"Bearer {api_key}"}
    resp = requests.post(f"{API_BASE}/games/{GAME_ID}/join", headers=headers, timeout=5)
    if resp.status_code not in [200, 400]:  # 400 = already joined
        print(f"Failed to join game: {resp.status_code} {resp.text}")
        return

    # Run tests
    test_burst(api_key, num_requests=30)

    print("\n  Waiting 3s for rate limit to reset...")
    time.sleep(3)

    test_sustained_rate(api_key, rate=8, duration=3)  # Should pass (under limit)

    time.sleep(3)

    test_sustained_rate(api_key, rate=15, duration=3)  # Should fail (over limit)

    time.sleep(3)

    test_recovery(api_key)

    # Cleanup
    requests.post(f"{API_BASE}/games/{GAME_ID}/leave", headers=headers, timeout=5)
    print("\n--- Done ---")


if __name__ == "__main__":
    main()
