#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
if [[ ! -f .env ]]; then
  echo 'Copy ops/staging/.env.example to .env and fill its values first.' >&2
  exit 1
fi
set -a
# shellcheck disable=SC1091
. ./.env
set +a

for name in TRACKER_AUTH_DB_PASSWORD TRACKER_APP_DB_PASSWORD \
  TRACKER_PLATFORM_DB_PASSWORD TRACKER_MAILER_DB_PASSWORD TRACKER_RETENTION_DB_PASSWORD; do
  value="${!name:-}"
  if [[ ! "$value" =~ ^[a-fA-F0-9]{48,}$ ]]; then
    echo "$name must be at least 48 hexadecimal characters." >&2
    exit 1
  fi
done

docker compose --env-file .env -f ops/staging/compose.yaml exec -T \
  -e TRACKER_AUTH_DB_PASSWORD \
  -e TRACKER_APP_DB_PASSWORD \
  -e TRACKER_PLATFORM_DB_PASSWORD \
  -e TRACKER_MAILER_DB_PASSWORD \
  -e TRACKER_RETENTION_DB_PASSWORD \
  db sh -c 'PGPASSWORD="$POSTGRES_PASSWORD" psql -X -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -f /provision/provision-roles.sql'
