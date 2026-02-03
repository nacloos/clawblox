#!/bin/bash
# Run migrations on local database
# Usage: ./scripts/migrate_local.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Load .env file if it exists
if [ -f "$PROJECT_ROOT/.env" ]; then
    export $(grep -v '^#' "$PROJECT_ROOT/.env" | xargs)
fi

if [ -z "$DATABASE_URL" ]; then
    echo "Error: DATABASE_URL not set in .env"
    exit 1
fi

echo "Running migrations on local database..."
cd "$PROJECT_ROOT"
sqlx migrate run

echo "Migrations completed successfully!"
