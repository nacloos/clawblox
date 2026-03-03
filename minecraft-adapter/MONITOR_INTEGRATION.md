# Monitor Integration Notes

A new standalone monitoring app was added at:

- `../monitor`

This keeps the adapter process thin while providing a rich UI for operations.

## Implemented

1. Web monitor server in `../monitor/server.mjs`
- Serves monitor UI
- Exposes tmux session listing (`/api/tmux/sessions`)
- Exposes tmux PTY websocket (`/term`) using `node-pty`

2. Monitor frontend in `../monitor/public/index.html`
- xterm.js terminal view for live tmux TUI rendering
- Adapter event stream panel (`/ws?session=<token>`)
- Agent stat cards (health, food, position, last event)

3. Standalone monitor package in `../monitor/package.json`
- Separate dependencies and lifecycle from adapter

## Why this architecture

- Preserves adapter simplicity
- Gives faithful TUI rendering (not line snapshots)
- Works cleanly with remote clusters via SSH tunneling
