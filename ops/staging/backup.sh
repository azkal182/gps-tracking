#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
if [[ ! -f .env ]]; then
  echo 'The staging .env file is missing.' >&2
  exit 1
fi
set -a
# shellcheck disable=SC1091
. ./.env
set +a

backup_dir="${STAGING_BACKUP_DIR:-/opt/gps-tracker/backups}"
mkdir -p "$backup_dir"
chmod 700 "$backup_dir"
umask 077
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup_file="$backup_dir/tracker-$stamp.dump"
docker compose --env-file .env -f ops/staging/compose.yaml exec -T db \
  sh -c 'PGPASSWORD="$POSTGRES_PASSWORD" pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom' \
  > "$backup_file"
if [[ ! -s "$backup_file" ]]; then
  rm -f "$backup_file"
  echo 'Database backup is empty.' >&2
  exit 1
fi
chmod 600 "$backup_file"
docker compose --env-file .env -f ops/staging/compose.yaml exec -T db \
  pg_restore --list < "$backup_file" >/dev/null
find "$backup_dir" -type f -name 'tracker-*.dump' -mtime +14 -delete
echo "Staging database backup verified: $backup_file"
