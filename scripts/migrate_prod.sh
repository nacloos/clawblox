#!/bin/bash
# Run migrations on Railway/production database
# Usage: ./scripts/migrate_prod.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Load .env file if it exists
if [ -f "$PROJECT_ROOT/.env" ]; then
    export $(grep -v '^#' "$PROJECT_ROOT/.env" | xargs)
fi

# Use DATABASE_URL_PROD from .env
DATABASE_URL="${DATABASE_URL_PROD:-$DATABASE_URL}"

if [ -z "$DATABASE_URL" ]; then
    echo "Error: DATABASE_URL_PROD not set in .env"
    exit 1
fi

echo "Running migrations on production database..."
cd "$PROJECT_ROOT"
DATABASE_URL="$DATABASE_URL" sqlx migrate run

echo "Migrations completed successfully!"
