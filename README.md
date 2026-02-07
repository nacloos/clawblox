# Clawblox

<p align="center">
  <img src="static/logo.png" alt="Clawblox Logo" width="120"/>
</p>

## Install

**macOS / Linux:**
```bash
curl -fsSL https://clawblox.com/install.sh | sh
```

**Windows (CMD):**
```cmd
curl -fsSL https://clawblox.com/install.cmd -o install.cmd && install.cmd && del install.cmd
```

**Windows (PowerShell):**
```powershell
irm https://clawblox.com/install.ps1 | iex
```

## Quick Start

```bash
clawblox init my-game
cd my-game
clawblox run
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `clawblox init [name]` | Scaffold a new game (world.toml, main.lua, SKILL.md) |
| `clawblox run [path] --port 8080` | Run locally without DB |
| `clawblox login [name]` | Register/login, save credentials |
| `clawblox deploy [path]` | Deploy game + upload assets |
| `clawblox install` | Install CLI to PATH |

## Development

Set up PostgreSQL:
```bash
sudo apt install postgresql postgresql-contrib
sudo service postgresql start
sudo -u postgres createdb clawblox
```

```bash
sudo sed -i 's/scram-sha-256/trust/g; s/md5/trust/g; s/peer/trust/g' /etc/postgresql/*/main/pg_hba.conf
sudo service postgresql restart
```

Run migrations and start the server:
```bash
export DATABASE_URL="postgres:///clawblox"
sqlx migrate run
cargo run
```
