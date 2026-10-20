#!/usr/bin/env bash
set -euo pipefail

if [[ "${APP_ENV:-}" != "development" ]]; then
  echo 'Set APP_ENV=development before running this script.' >&2
  exit 1
fi
: "${POSTGRES_DB:?POSTGRES_DB is required}"
: "${POSTGRES_USER:?POSTGRES_USER is required}"
: "${TRACKER_AUTH_DB_PASSWORD:?TRACKER_AUTH_DB_PASSWORD is required}"
: "${TRACKER_APP_DB_PASSWORD:?TRACKER_APP_DB_PASSWORD is required}"

for password in "$TRACKER_AUTH_DB_PASSWORD" "$TRACKER_APP_DB_PASSWORD"; do
  if [[ ! "$password" =~ ^[A-Fa-f0-9]{32,}$ ]]; then
    echo 'Runtime role passwords must be at least 32 hexadecimal characters.' >&2
    exit 1
  fi
done

# The owner account is used only to provision these local login roles. Runtime
# connections inherit the narrower capability roles created by migrations.
docker compose exec -T db psql -X -v ON_ERROR_STOP=1 \
  -U "$POSTGRES_USER" -d "$POSTGRES_DB" <<SQL
DO \$provision\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='gps_dev_auth') THEN
    EXECUTE format('CREATE ROLE gps_dev_auth LOGIN INHERIT NOBYPASSRLS PASSWORD %L', '$TRACKER_AUTH_DB_PASSWORD');
  ELSE
    EXECUTE format('ALTER ROLE gps_dev_auth LOGIN INHERIT NOBYPASSRLS PASSWORD %L', '$TRACKER_AUTH_DB_PASSWORD');
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='gps_dev_app') THEN
    EXECUTE format('CREATE ROLE gps_dev_app LOGIN INHERIT NOBYPASSRLS PASSWORD %L', '$TRACKER_APP_DB_PASSWORD');
  ELSE
    EXECUTE format('ALTER ROLE gps_dev_app LOGIN INHERIT NOBYPASSRLS PASSWORD %L', '$TRACKER_APP_DB_PASSWORD');
  END IF;
END
\$provision\$;
GRANT gps_tracking_auth TO gps_dev_auth;
GRANT gps_tracking_app TO gps_dev_app;
SQL

python3 - <<'PY'
import os
from pathlib import Path

env_path = Path('.env')
lines = env_path.read_text().splitlines()
host = '127.0.0.1'
database = os.environ['POSTGRES_DB']
port = os.environ['POSTGRES_PORT']
updates = {
    'DATABASE_URL': f"postgres://gps_dev_auth:{os.environ['TRACKER_AUTH_DB_PASSWORD']}@{host}:{port}/{database}?sslmode=disable",
    'TRACKING_DATABASE_URL': f"postgres://gps_dev_app:{os.environ['TRACKER_APP_DB_PASSWORD']}@{host}:{port}/{database}?sslmode=disable",
}
seen = set()
result = []
for line in lines:
    key = line.split('=', 1)[0] if '=' in line and not line.lstrip().startswith('#') else None
    if key in updates:
        result.append(f'{key}={updates[key]}')
        seen.add(key)
    else:
        result.append(line)
for key, value in updates.items():
    if key not in seen:
        result.append(f'{key}={value}')
env_path.write_text('\n'.join(result) + '\n')
env_path.chmod(0o600)
PY

echo 'Development runtime roles are ready.'
