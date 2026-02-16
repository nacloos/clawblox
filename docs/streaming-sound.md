# Streaming Sound (Server Authoritative)

Streaming speech is server-authoritative in local CLI runtime.
Frontend playback is presentational only and does not control turn release.

## Overview

- Audio bytes are streamed through `POST /audio`.
- The server tracks per-stream metadata (`stream_id`, speaker, bytes, format, done flag).
- On final chunk, the server computes expected playback duration from PCM metadata.
- The server schedules completion and queues `SpeechFinished` / `AudioEnded` itself.
- Frontend no longer sends `audio_done` for authority.

## Duration Math

For PCM streams:

`duration_seconds = total_audio_bytes / (sample_rate_hz * channels * bytes_per_sample) / playback_speed`

For the default local pipeline:
- `sample_rate_hz = 24000`
- `channels = 1`
- `bytes_per_sample = 2`

## Data Flow

```
Agent claims speaking turn (game-level contract, e.g. SpeechBus Claim)
  -> game script sets current speaker lock

Agent streams audio chunks via POST /audio
  -> server tracks byte counts and broadcasts audio chunks to spectators

Agent sends final chunk with done=true (+ optional speech_text)
  -> server computes expected playback end from bytes and audio format
  -> server schedules completion on server clock

At predicted end:
  -> server emits playback_done event
  -> server publishes speech text (if provided)
  -> server queues SpeechFinished and AudioEnded inputs

Game script handles SpeechFinished/AudioEnded
  -> releases speaker lock
```

## API Contract (Local Runtime)

### `POST /audio`

Request body fields:
- `stream_id` (string)
- `seq` (number)
- `data` (base64 PCM payload)
- `done` (boolean)
- `speech_text` (optional string, usually on final chunk)
- `sample_rate_hz` (optional number, default 24000)
- `channels` (optional number, default 1)
- `bytes_per_sample` (optional number, default 2)
- `playback_speed` (optional number, default 1.0)

### `GET /spectate/ws`

Receives:
- `audio_chunk` events (for frontend playback)
- `speech` events
- `playback_done` events
- spectator state snapshots

## No-Audio Mode

No-audio agents are supported by the same authority model:
- claim turn
- finalize speech with `total_audio_bytes = 0`
- server resolves duration as zero and releases on server path immediately

No frontend callbacks are required.

## Why This Improves Roblox Parity

- Turn ownership/release is server-authoritative.
- Client playback status is not authoritative.
- Completion timing is deterministic from server-known stream data.
- Transport remains explicit, but authority boundaries match Roblox-style server control.
