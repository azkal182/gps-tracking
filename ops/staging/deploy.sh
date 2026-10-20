#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
requested_image_tag="${IMAGE_TAG:?IMAGE_TAG must be the immutable Git commit SHA}"
[[ "$requested_image_tag" =~ ^[a-f0-9]{40}$ ]] || { echo 'IMAGE_TAG must be a 40-character commit SHA.' >&2; exit 1; }
[[ -f .env ]] || { echo 'Staging .env is missing.' >&2; exit 1; }
set -a
# shellcheck disable=SC1091
. ./.env
set +a
export IMAGE_TAG="$requested_image_tag"

docker compose --env-file .env -f ops/staging/compose.yaml config --quiet
docker compose --profile tools --env-file .env -f ops/staging/compose.yaml pull api portal migrate retention
docker compose --env-file .env -f ops/staging/compose.yaml up -d db

ready=0
for _ in $(seq 1 60); do
  if docker compose --env-file .env -f ops/staging/compose.yaml exec -T db \
    sh -c 'pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB"' >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 2
done
[[ "$ready" == 1 ]] || { echo 'PostgreSQL did not become ready.' >&2; exit 1; }

./ops/staging/backup.sh
docker compose --profile tools --env-file .env -f ops/staging/compose.yaml run --rm migrate
./ops/staging/provision-roles.sh
docker compose --env-file .env -f ops/staging/compose.yaml up -d api portal retention

ready=0
for _ in $(seq 1 60); do
  if curl --fail --silent --output /dev/null "https://${STAGING_DOMAIN}/readyz"; then
    ready=1
    break
  fi
  sleep 5
done
[[ "$ready" == 1 ]] || { echo 'Staging readiness check failed; inspect container logs.' >&2; exit 1; }
docker compose --env-file .env -f ops/staging/compose.yaml ps
echo "Staging deployed: $IMAGE_TAG"
