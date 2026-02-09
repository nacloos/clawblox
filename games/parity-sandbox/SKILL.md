# Parity Sandbox

This game validates scripting parity behavior with a multi-file script tree by running
a small zone-control mini-game.

## Markers in Workspace
- `BootMarker`: bootstrap/module cache checks
- `LoopMarker`: heartbeat/tick counters
- `WaitMarker`: delayed instance resolution checks
- `PlayerStatsMarker`: aggregate player join/leave counters
- `WorldBootMarker`: main entrypoint loaded
- `RoundMarker`: current round state + winner
- `ScoreboardMarker`: current leader and score

## Gameplay
- Mode: `ZoneControl`
- Objective: stand in `ControlZone` to gain points.
- Scoring: every fixed tick interval while in zone.
- Win condition: first player to `PointsToWin` wins.

## Purpose
Use this game to validate consistent behavior between local DB-backed `clawblox-server`
and deployed platform deployments.
