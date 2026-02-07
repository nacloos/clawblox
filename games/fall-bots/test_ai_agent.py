#!/usr/bin/env python3
"""
AI-powered agent for Fall Bots obstacle course race.
Uses Claude to make decisions based on game observations.
"""

import argparse
import json
import os
import random
import sys
import threading
import time
from pathlib import Path

import anthropic
import requests

# Load .env file if present (project root is ../../)
env_path = Path(__file__).parent.parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())

API_BASE = os.getenv("CLAWBLOX_API_URL", "http://localhost:8080/api/v1")
API_BASE_PROD = os.getenv("CLAWBLOX_API_URL_PROD", "https://clawblox.com/api/v1")
GAME_ID = "a0000000-0000-0000-0000-000000000008"  # Fall Bots
KEYS_CACHE = Path("/tmp/clawblox_fallbots_keys.json")

# Load the game's SKILL.md as context for the LLM
SKILL_MD = (Path(__file__).parent / "SKILL.md").read_text()


def load_cached_keys(api_base: str) -> list[str]:
    if KEYS_CACHE.exists():
        try:
            data = json.loads(KEYS_CACHE.read_text())
            return data.get("by_api", {}).get(api_base, [])
        except Exception:
            pass
    return []


def save_cached_keys(api_base: str, keys: list[str]):
    data = {"by_api": {}}
    if KEYS_CACHE.exists():
        try:
            existing = json.loads(KEYS_CACHE.read_text())
            if isinstance(existing.get("by_api"), dict):
                data["by_api"] = existing["by_api"]
        except Exception:
            pass
    data["by_api"][api_base] = keys
    KEYS_CACHE.write_text(json.dumps(data))


def register_agent(api_base: str, name: str) -> str | None:
    try:
        resp = requests.post(
            f"{api_base}/agents/register",
            json={"name": name, "description": "AI-powered Fall Bots agent"},
            timeout=10,
        )
        if resp.status_code == 200:
            return resp.json()["agent"]["api_key"]
    except Exception as e:
        print(f"Registration error: {e}")
    return None


def get_api_keys(api_base: str, num_needed: int) -> list[str]:
    keys = load_cached_keys(api_base)
    while len(keys) < num_needed:
        name = f"fallbots_ai_{random.randint(1000, 9999)}"
        print(f"Registering {name}...")
        key = register_agent(api_base, name)
        if key:
            keys.append(key)
            print(f"  OK: {key[:20]}...")
    save_cached_keys(api_base, keys)
    return keys[:num_needed]


def summarize_observation(obs: dict) -> str:
    """Create a compact text summary of the observation for the LLM."""
    player = obs["player"]
    pos = player["position"]
    attrs = player.get("attributes", {})
    entities = obs.get("world", {}).get("entities", [])

    lines = [
        f"Position: ({pos[0]:.1f}, {pos[1]:.1f}, {pos[2]:.1f})",
        f"Status: {attrs.get('Status', '?')} | Section: {int(attrs.get('Section', 0))} | Time left: {attrs.get('TimeRemaining', 0):.0f}s",
    ]

    # Group entities by type for compact summary
    doors = []
    spinbars = []
    platforms = []
    pendulums = []
    crown = None

    for e in entities:
        name = e.get("name", "")
        epos = e["position"]
        if name.startswith("Door_"):
            breakable = e.get("attributes", {}).get("Breakable", False)
            doors.append(f"  {name} ({epos[0]:.0f},{epos[2]:.0f}) {'BREAKABLE' if breakable else 'SOLID'}")
        elif name.startswith("SpinBar_"):
            spinbars.append(f"  {name} ({epos[0]:.0f},{epos[1]:.0f},{epos[2]:.0f}) size={e['size']}")
        elif name.startswith("Platform_"):
            color = e.get("color", [0, 0, 0])
            # Blue = visible/safe, Red-ish = warning/hidden
            is_safe = color[2] > 0.5  # blue channel high = visible
            platforms.append(f"  {name} ({epos[0]:.0f},{epos[1]:.0f},{epos[2]:.0f}) {'VISIBLE' if is_safe else 'WARNING/HIDDEN'}")
        elif name.startswith("Pendulum_"):
            pendulums.append(f"  {name} ({epos[0]:.0f},{epos[1]:.0f},{epos[2]:.0f})")
        elif name == "Crown":
            crown = f"  Crown at ({epos[0]:.0f},{epos[1]:.0f},{epos[2]:.0f})"

    section = int(attrs.get("Section", 1))
    if section == 1 and doors:
        lines.append("Nearby doors:")
        # Only show doors in the next ~20 studs ahead
        for d in doors:
            lines.append(d)
    if section == 2 and spinbars:
        lines.append("Spinning bars:")
        for s in spinbars:
            lines.append(s)
    if section == 3 and platforms:
        lines.append("Platforms:")
        for p in platforms:
            lines.append(p)
    if section == 4:
        if pendulums:
            lines.append("Pendulums:")
            for p in pendulums:
                lines.append(p)
        if crown:
            lines.append(crown)

    return "\n".join(lines)


def run_ai_agent(
    agent_id: int,
    api_key: str,
    api_base: str,
    game_id: str,
    stop_event: threading.Event,
    model: str = "claude-haiku-4-5-20251001",
):
    """Run a single AI-powered agent."""
    prefix = f"[AI-{agent_id}]"
    headers = {"Authorization": f"Bearer {api_key}"}
    client = anthropic.Anthropic()

    # Leave any existing games
    try:
        resp = requests.get(f"{api_base}/games", headers=headers, timeout=5)
        for g in resp.json().get("games", []):
            requests.post(f"{api_base}/games/{g['id']}/leave", headers=headers, timeout=5)
    except Exception:
        pass

    # Join game
    resp = requests.post(f"{api_base}/games/{game_id}/join", headers=headers, timeout=5)
    if resp.status_code != 200:
        print(f"{prefix} Failed to join: {resp.text}")
        return
    print(f"{prefix} Joined game!")

    time.sleep(0.5)

    system_prompt = f"""You are an AI agent playing Fall Bots, an obstacle course race game.

{SKILL_MD}

## Your Task

Each turn you receive a game observation summary. Respond with ONLY a JSON action.

Available actions:
- Move forward: {{"action": "MoveTo", "position": [x, y, z]}}
- Jump: {{"action": "Jump"}}

## Key Rules
- The course goes along the Z-axis (Z=0 to Z=300). ALWAYS move toward higher Z values.
- You are 2.5 studs wide. Doors are 5.5 studs wide at X positions: -12, -6, 0, 6, 12.
- Green/BREAKABLE doors shatter on contact. Red/SOLID doors block you. Always aim for BREAKABLE doors.
- In Section 2, spinning bars rotate. Move between them when they're away. Jump over low bars (height <= 2).
- In Section 3, only move to VISIBLE platforms (blue color). Avoid WARNING/HIDDEN ones.
- In Section 4, pendulums swing left-right. Move through gaps when pendulum X is far from 0.
- The Crown is at Z=295. Reach it to win!

Respond with ONLY valid JSON, no explanation."""

    conversation_history = []
    last_action_time = 0
    decision_interval = 1.0  # seconds between LLM decisions

    try:
        while not stop_event.is_set():
            # Observe
            try:
                resp = requests.get(
                    f"{api_base}/games/{game_id}/observe",
                    headers=headers,
                    timeout=5,
                )
                if resp.status_code != 200:
                    time.sleep(0.5)
                    continue
            except requests.exceptions.RequestException:
                time.sleep(0.5)
                continue

            obs = resp.json()
            attrs = obs["player"].get("attributes", {})
            status = attrs.get("Status", "waiting")

            # Skip LLM call if not racing
            if status in ("waiting", "countdown"):
                print(f"{prefix} Status: {status}, waiting...")
                time.sleep(1)
                continue

            if status in ("finished", "dnf"):
                pos_str = f"#{int(attrs.get('FinishPosition', 0))}" if status == "finished" else "DNF"
                print(f"{prefix} Race over! Result: {pos_str}")
                break

            # Throttle LLM calls
            now = time.time()
            if now - last_action_time < decision_interval:
                time.sleep(0.1)
                continue

            # Summarize observation
            summary = summarize_observation(obs)
            print(f"{prefix} {summary.splitlines()[0]} | {summary.splitlines()[1]}")

            # Ask the LLM for a decision
            # Keep conversation short (last 4 turns max) to save tokens
            conversation_history.append({"role": "user", "content": summary})
            if len(conversation_history) > 8:
                conversation_history = conversation_history[-8:]

            try:
                llm_resp = client.messages.create(
                    model=model,
                    max_tokens=150,
                    system=system_prompt,
                    messages=conversation_history,
                )
                action_text = llm_resp.content[0].text.strip()

                # Parse action
                # Handle markdown code blocks
                if action_text.startswith("```"):
                    action_text = "\n".join(action_text.split("\n")[1:-1])
                action = json.loads(action_text)

                conversation_history.append({"role": "assistant", "content": action_text})

                # Execute action
                if action.get("action") == "MoveTo":
                    pos = action["position"]
                    payload = {"type": "MoveTo", "data": {"position": pos}}
                    requests.post(
                        f"{api_base}/games/{game_id}/input",
                        headers=headers,
                        json=payload,
                        timeout=5,
                    )
                    print(f"{prefix} -> MoveTo ({pos[0]:.0f}, {pos[1]:.0f}, {pos[2]:.0f})")
                elif action.get("action") == "Jump":
                    requests.post(
                        f"{api_base}/games/{game_id}/input",
                        headers=headers,
                        json={"type": "Jump"},
                        timeout=5,
                    )
                    print(f"{prefix} -> Jump")
                else:
                    print(f"{prefix} Unknown action: {action}")

            except (json.JSONDecodeError, KeyError, IndexError) as e:
                print(f"{prefix} LLM parse error: {e} | raw: {action_text[:100]}")
            except anthropic.APIError as e:
                print(f"{prefix} API error: {e}")
                time.sleep(2)

            last_action_time = time.time()

    finally:
        requests.post(f"{api_base}/games/{game_id}/leave", headers=headers, timeout=5)
        print(f"{prefix} Left game.")


def main():
    parser = argparse.ArgumentParser(description="AI-powered Fall Bots agent")
    parser.add_argument("-n", "--num-agents", type=int, default=2, help="Number of agents (default 2, minimum for race)")
    parser.add_argument("--model", type=str, default="claude-haiku-4-5-20251001", help="Claude model to use")
    parser.add_argument("--prod", action="store_true", help="Run against production")
    parser.add_argument("--interval", type=float, default=1.0, help="Seconds between LLM decisions (default 1.0)")
    args = parser.parse_args()

    api_base = API_BASE_PROD if args.prod else API_BASE
    print(f"API: {api_base}")
    print(f"Model: {args.model}")
    print(f"Agents: {args.num_agents}")
    print(f"Decision interval: {args.interval}s")
    print("-" * 60)

    if not os.getenv("ANTHROPIC_API_KEY"):
        print("Error: ANTHROPIC_API_KEY not set")
        sys.exit(1)

    api_keys = get_api_keys(api_base, args.num_agents)
    print(f"Got {len(api_keys)} API key(s)")

    stop_event = threading.Event()
    threads = []

    for i in range(args.num_agents):
        t = threading.Thread(
            target=run_ai_agent,
            args=(i, api_keys[i], api_base, GAME_ID, stop_event, args.model),
        )
        t.daemon = True
        t.start()
        threads.append(t)
        time.sleep(0.3)

    try:
        while True:
            time.sleep(1)
            # Check if all threads are done
            if all(not t.is_alive() for t in threads):
                print("\nAll agents finished.")
                break
    except KeyboardInterrupt:
        print("\nStopping...")
    finally:
        stop_event.set()
        for t in threads:
            t.join(timeout=3)


if __name__ == "__main__":
    main()
