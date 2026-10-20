#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
requested_image_tag="${IMAGE_TAG:?Set IMAGE_TAG to the previously verified commit SHA}"
[[ "$requested_image_tag" =~ ^[a-f0-9]{40}$ ]] || { echo 'IMAGE_TAG must be a 40-character commit SHA.' >&2; exit 1; }
set -a
# shellcheck disable=SC1091
. ./.env
set +a
export IMAGE_TAG="$requested_image_tag"
docker compose --env-file .env -f ops/staging/compose.yaml pull api portal retention
docker compose --env-file .env -f ops/staging/compose.yaml up -d api portal retention
curl --fail --silent --show-error "https://${STAGING_DOMAIN}/readyz"
docker compose --env-file .env -f ops/staging/compose.yaml ps
echo "Application image rollback completed: $IMAGE_TAG (database was not rolled back)."
