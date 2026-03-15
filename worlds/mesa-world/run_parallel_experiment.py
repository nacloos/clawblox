#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import os
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import TextIO


RUNTIME_SECONDS = 3600


@dataclass
class Condition:
    name: str
    template_dir: Path
    port: int
    agent_name: str
    world_root: Path
    logs_dir: Path
    recordings_dir: Path
    agent_root: Path
    workspace_dir: Path
    session_dir: Path
    world_log: Path
    agent_log: Path
    world_proc: subprocess.Popen | None = None
    agent_proc: subprocess.Popen | None = None
    world_log_handle: TextIO | None = None
    agent_log_handle: TextIO | None = None


def parse_args() -> argparse.Namespace:
    script_dir = Path(__file__).resolve().parent
    root_dir = script_dir.parent.parent
    default_run_id = f"parallel-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}"

    parser = argparse.ArgumentParser(
        description="Run 2 naive + 2 experienced agents in parallel for a fixed 1-hour budget."
    )
    parser.add_argument("--run-id", default=default_run_id)
    parser.add_argument("--base-port", type=int, default=18080)
    parser.add_argument("--health-timeout", type=int, default=30)
    parser.add_argument(
        "--goal",
        default="Goal: build the tallest stable tower possible.",
        help="Startup instructions passed to each agent.",
    )
    parser.add_argument(
        "--naive-template",
        type=Path,
        default=root_dir / "agent" / "template" / "agent",
    )
    parser.add_argument(
        "--experienced-workspace-a",
        type=Path,
        default=script_dir
        / "results"
        / "20260309T195534Z"
        / "worlds"
        / "world-0"
        / "agents"
        / "mesa-r20260309T195534Z-w0-a0"
        / "workspace",
    )
    parser.add_argument(
        "--experienced-workspace-b",
        type=Path,
        default=script_dir
        / "results"
        / "20260309T195534Z"
        / "worlds"
        / "world-1"
        / "agents"
        / "mesa-r20260309T195534Z-w1-a0"
        / "workspace",
    )
    return parser.parse_args()


def load_dotenv(dotenv_path: Path) -> None:
    if not dotenv_path.exists():
        return
    for raw in dotenv_path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip("'").strip('"')
        os.environ.setdefault(key, value)


def find_clawblox_bin(root_dir: Path) -> str:
    candidate = root_dir / "target" / "release" / "clawblox"
    if candidate.exists():
        return str(candidate)
    in_path = shutil.which("clawblox")
    if in_path:
        return in_path
    raise RuntimeError("clawblox binary not found in target/release or PATH.")


def supports_record_dir(clawblox_bin: str) -> bool:
    result = subprocess.run(
        [clawblox_bin, "run", "--help"],
        check=False,
        capture_output=True,
        text=True,
    )
    return "--record-dir" in (result.stdout + result.stderr)


def is_port_in_use(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.2)
        return sock.connect_ex(("127.0.0.1", port)) == 0


def wait_for_server(port: int, timeout_s: int, stop_event: threading.Event | None = None) -> bool:
    deadline = time.time() + timeout_s
    url = f"http://localhost:{port}/skill.md"
    while time.time() < deadline:
        if stop_event and stop_event.is_set():
            return False
        try:
            with urllib.request.urlopen(url, timeout=1.5) as resp:
                if resp.status == 200:
                    return True
        except (urllib.error.URLError, TimeoutError):
            time.sleep(1.0)
    return False


def sanitize_name(value: str) -> str:
    out = []
    allowed = set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-")
    for ch in value:
        out.append(ch if ch in allowed else "_")
    return "".join(out)


def stop_process(proc: subprocess.Popen | None) -> None:
    if proc is None or proc.poll() is not None:
        return

    try:
        pgid = os.getpgid(proc.pid)
    except ProcessLookupError:
        return

    def send_group(sig: int) -> None:
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            pass

    send_group(signal.SIGINT)
    try:
        proc.wait(timeout=5)
        return
    except subprocess.TimeoutExpired:
        pass
    send_group(signal.SIGTERM)
    try:
        proc.wait(timeout=3)
        return
    except subprocess.TimeoutExpired:
        pass
    send_group(signal.SIGKILL)
    proc.wait(timeout=3)


def finalize_recording_db(recording: Path) -> None:
    if not recording.exists():
        return
    wal_file = Path(f"{recording}-wal")
    shm_file = Path(f"{recording}-shm")
    try:
        with sqlite3.connect(str(recording)) as conn:
            conn.execute("PRAGMA wal_checkpoint(TRUNCATE);")
    except sqlite3.Error:
        return
    if wal_file.exists() and wal_file.stat().st_size == 0:
        wal_file.unlink(missing_ok=True)
    if shm_file.exists() and not wal_file.exists():
        shm_file.unlink(missing_ok=True)


def run_analyzer(world_dir: Path, recording: Path, analysis_dir: Path, agent_name: str) -> None:
    analyzer = world_dir / "analysis" / "analyze_recording.py"
    uv = shutil.which("uv")
    if uv:
        cmd = [
            uv,
            "run",
            str(analyzer),
            str(recording),
            "--output-dir",
            str(analysis_dir),
            "--agent-name",
            agent_name,
        ]
    else:
        cmd = [
            sys.executable,
            str(analyzer),
            str(recording),
            "--output-dir",
            str(analysis_dir),
            "--agent-name",
            agent_name,
        ]
    subprocess.run(cmd, cwd=str(world_dir), check=True)


def close_handle(handle: TextIO | None) -> None:
    if handle:
        handle.flush()
        handle.close()


def main() -> int:
    args = parse_args()
    world_dir = Path(__file__).resolve().parent
    root_dir = world_dir.parent.parent
    load_dotenv(root_dir / ".env")

    if not os.environ.get("ANTHROPIC_API_KEY"):
        raise RuntimeError("ANTHROPIC_API_KEY is not set.")

    clawblox_bin = find_clawblox_bin(root_dir)
    if not supports_record_dir(clawblox_bin):
        raise RuntimeError(f"'{clawblox_bin} run' does not support --record-dir.")

    agent_script = root_dir / "agent" / "run_agent.sh"
    if not agent_script.exists():
        raise RuntimeError(f"agent script missing at {agent_script}")

    naive_template = args.naive_template.resolve()
    exp_a = args.experienced_workspace_a.resolve()
    exp_b = args.experienced_workspace_b.resolve()
    for template in (naive_template, exp_a, exp_b):
        if not template.is_dir():
            raise RuntimeError(f"template/workspace not found: {template}")

    run_id = sanitize_name(args.run_id)
    run_dir = world_dir / "results" / run_id
    if run_dir.exists():
        raise RuntimeError(f"run directory already exists: {run_dir}")
    (run_dir / "worlds").mkdir(parents=True, exist_ok=False)
    (world_dir / "results" / "latest_run.txt").write_text(str(run_dir) + "\n")

    conditions: list[Condition] = []
    defs = [
        ("naive_a", naive_template, args.base_port),
        ("naive_b", naive_template, args.base_port + 1),
        ("experienced_a", exp_a, args.base_port + 2),
        ("experienced_b", exp_b, args.base_port + 3),
    ]
    for name, template, port in defs:
        if is_port_in_use(port):
            raise RuntimeError(f"port {port} is already in use")
        agent_name = sanitize_name(f"mesa-{name}-r{run_id}")
        world_root = run_dir / "worlds" / name
        logs_dir = world_root / "logs"
        recordings_dir = world_root / "recordings"
        agent_root = world_root / "agents" / agent_name
        workspace_dir = agent_root / "workspace"
        session_dir = agent_root / "session"
        logs_dir.mkdir(parents=True, exist_ok=True)
        recordings_dir.mkdir(parents=True, exist_ok=True)
        (agent_root / "logs").mkdir(parents=True, exist_ok=True)
        workspace_dir.mkdir(parents=True, exist_ok=True)
        session_dir.mkdir(parents=True, exist_ok=True)
        conditions.append(
            Condition(
                name=name,
                template_dir=template,
                port=port,
                agent_name=agent_name,
                world_root=world_root,
                logs_dir=logs_dir,
                recordings_dir=recordings_dir,
                agent_root=agent_root,
                workspace_dir=workspace_dir,
                session_dir=session_dir,
                world_log=logs_dir / "world.log",
                agent_log=agent_root / "logs" / "agent.log",
            )
        )

    print(f"Starting parallel experiment: {run_id}")
    print(f"Run dir: {run_dir}")
    print(f"Runtime: {RUNTIME_SECONDS}s (fixed)")

    stop_requested = threading.Event()
    interrupted = False
    previous_sigint = signal.getsignal(signal.SIGINT)
    previous_sigterm = signal.getsignal(signal.SIGTERM)

    def request_stop(signum: int, _frame: object) -> None:
        nonlocal interrupted
        interrupted = True
        if not stop_requested.is_set():
            print(f"Received signal {signum}; stopping all processes...")
        stop_requested.set()

    signal.signal(signal.SIGINT, request_stop)
    signal.signal(signal.SIGTERM, request_stop)

    try:
        for c in conditions:
            if stop_requested.is_set():
                break
            c.world_log_handle = c.world_log.open("w")
            c.world_proc = subprocess.Popen(
                [
                    clawblox_bin,
                    "run",
                    str(world_dir),
                    "--port",
                    str(c.port),
                    "--record",
                    "--record-dir",
                    str(c.recordings_dir),
                ],
                cwd=str(root_dir),
                stdout=c.world_log_handle,
                stderr=subprocess.STDOUT,
                env=os.environ.copy(),
                start_new_session=True,
            )
            if not wait_for_server(c.port, args.health_timeout, stop_event=stop_requested):
                if stop_requested.is_set():
                    interrupted = True
                    break
                raise RuntimeError(f"world failed health check: {c.name} on port {c.port}")
            print(f"[{c.name}] world ready on {c.port}")

            c.agent_log_handle = c.agent_log.open("w")
            agent_env = os.environ.copy()
            startup_instructions = (
                "First fetch capabilities with:\n"
                f'curl -sS -H "X-Session: <session-key>" http://localhost:{c.port}/skill.md\n'
                "Use the session key shown above in this prompt.\n\n"
                f"Then follow this objective:\n{args.goal}"
            )
            agent_env.update(
                {
                    "WORLD_BASE_URL": f"http://localhost:{c.port}",
                    "WORLD_AGENT_NAME": c.agent_name,
                    "WORKSPACE_DIR": str(c.workspace_dir),
                    "SESSION_DIR": str(c.session_dir),
                    "TEMPLATE_DIR": str(c.template_dir),
                    "WORLD_STARTUP_INSTRUCTIONS": startup_instructions,
                }
            )
            c.agent_proc = subprocess.Popen(
                ["bash", str(agent_script)],
                cwd=str(root_dir),
                stdout=c.agent_log_handle,
                stderr=subprocess.STDOUT,
                env=agent_env,
                start_new_session=True,
            )
            print(f"[{c.name}] agent started: {c.agent_name}")

        if not interrupted:
            print(f"All conditions running in parallel for {RUNTIME_SECONDS} seconds...")
            end_time = time.time() + RUNTIME_SECONDS
            while time.time() < end_time:
                if stop_requested.is_set():
                    interrupted = True
                    break
                for c in conditions:
                    if c.world_proc and c.world_proc.poll() is not None:
                        raise RuntimeError(f"world process exited early: {c.name}")
                    if c.agent_proc and c.agent_proc.poll() is not None:
                        raise RuntimeError(f"agent process exited early: {c.name}")
                time.sleep(2.0)
    finally:
        for c in conditions:
            stop_process(c.agent_proc)
        for c in conditions:
            stop_process(c.world_proc)
        for c in conditions:
            close_handle(c.agent_log_handle)
            close_handle(c.world_log_handle)
        signal.signal(signal.SIGINT, previous_sigint)
        signal.signal(signal.SIGTERM, previous_sigterm)

    if interrupted:
        print("Interrupted: child processes were stopped. Skipping analysis.")
        return 130

    summary_csv = run_dir / "summary.csv"
    with summary_csv.open("w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(
            [
                "condition",
                "agent_name",
                "recording",
                "max_agent_y",
                "max_stack_seen",
                "max_agent_y_tick",
                "max_stack_tick",
                "summary_json",
            ]
        )

        for c in conditions:
            recordings = sorted(c.recordings_dir.glob("*.clawrec"))
            if not recordings:
                print(f"[warn] no recording for {c.name}")
                continue
            recording = recordings[-1]
            finalize_recording_db(recording)

            analysis_dir = c.world_root / "analysis"
            analysis_dir.mkdir(parents=True, exist_ok=True)
            run_analyzer(world_dir, recording, analysis_dir, c.agent_name)

            summary_path = analysis_dir / "summary.json"
            if not summary_path.exists():
                print(f"[warn] missing summary for {c.name}: {summary_path}")
                continue

            summary = json.loads(summary_path.read_text())
            writer.writerow(
                [
                    c.name,
                    c.agent_name,
                    str(recording),
                    summary.get("max_agent_y", ""),
                    summary.get("max_stack_seen", ""),
                    summary.get("max_agent_y_tick", ""),
                    summary.get("max_stack_tick", ""),
                    str(summary_path),
                ]
            )

    print("Done.")
    print(f"Run dir: {run_dir}")
    print(f"Summary CSV: {summary_csv}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        raise SystemExit(1)
