#!/usr/bin/env bash
set -Eeuo pipefail

: "${SUPABASE_ACCESS_TOKEN:?set SUPABASE_ACCESS_TOKEN}"
: "${SUPABASE_PROJECT_REF:?set SUPABASE_PROJECT_REF}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
migrations_dir="$root/apps/api/migrations"
api_url="https://api.supabase.com/v1/projects/${SUPABASE_PROJECT_REF}/database/query"

query() {
  local sql="$1"
  local read_only="${2:-false}"
  jq -n --arg query "$sql" --argjson read_only "$read_only" \
    '{query: $query, read_only: $read_only}' |
    curl --fail-with-body --silent --show-error \
      -H "Authorization: Bearer ${SUPABASE_ACCESS_TOKEN}" \
      -H 'Content-Type: application/json' \
      --data-binary @- "$api_url"
}

rows() {
  jq -r 'if type == "object" and has("result") then .result else . end'
}

query "
  CREATE SCHEMA IF NOT EXISTS mira;
  CREATE TABLE IF NOT EXISTS mira.schema_migrations (
    version TEXT PRIMARY KEY,
    checksum TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );
  REVOKE ALL ON SCHEMA mira FROM PUBLIC, anon, authenticated, service_role;
  REVOKE ALL ON mira.schema_migrations FROM PUBLIC, anon, authenticated, service_role;
" >/dev/null

shopt -s nullglob
for migration in "$migrations_dir"/*.sql; do
  version="$(basename "$migration" .sql)"
  checksum="$(sha256sum "$migration" | awk '{print $1}')"
  stored="$(query "SELECT checksum FROM mira.schema_migrations WHERE version = '${version}';" true | rows | jq -r '.[0].checksum // empty')"

  if [[ -n "$stored" ]]; then
    [[ "$stored" == "$checksum" ]] || {
      echo "checksum mismatch for $version" >&2
      exit 1
    }
    echo "already applied: $version"
    continue
  fi

  sql="$(<"$migration")"
  query "
    BEGIN;
    SELECT pg_advisory_xact_lock(hashtext('mira-schema-migrations'));
    ${sql}
    INSERT INTO mira.schema_migrations (version, checksum)
    VALUES ('${version}', '${checksum}')
    ON CONFLICT (version) DO NOTHING;
    COMMIT;
  " >/dev/null

  stored="$(query "SELECT checksum FROM mira.schema_migrations WHERE version = '${version}';" true | rows | jq -r '.[0].checksum // empty')"
  [[ "$stored" == "$checksum" ]] || {
    echo "migration was not recorded: $version" >&2
    exit 1
  }
  echo "applied: $version"
done
