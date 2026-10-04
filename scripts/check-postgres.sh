#!/usr/bin/env bash
set -euo pipefail

# Usa siempre la base desechable mira_test. Sin DSN, levanta PostgreSQL local
# con un puerto aleatorio ligado exclusivamente a loopback.
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
container_id=""
restore_created=""
ci_backup_dir=""
drill_dir="$(mktemp -d)"
cleanup() {
  if [[ -n "$ci_backup_dir" ]]; then
    docker exec "$TEST_POSTGRES_CONTAINER_ID" rm -rf "$ci_backup_dir" >/dev/null
  fi
  if [[ -n "$restore_created" && -z "$container_id" ]]; then
    psql "$TEST_POSTGRES_DATABASE_URL" -v ON_ERROR_STOP=1 -qc 'DROP DATABASE mira_restore_test' >/dev/null
  fi
  if [[ -n "$container_id" ]]; then docker rm -f "$container_id" >/dev/null; fi
  rm -rf "$drill_dir"
}
trap cleanup EXIT
if [[ -z "${TEST_POSTGRES_DATABASE_URL:-}" ]]; then
  container_id="$(docker run --detach --rm --publish 127.0.0.1::5432 \
    --env POSTGRES_DB=mira_test --env POSTGRES_USER=mira_test \
    --env POSTGRES_HOST_AUTH_METHOD=trust \
    --volume "$repo_dir:/repo:ro" --volume "$drill_dir:/backup" postgres:16)"
  for attempt in {1..30}; do
    if docker exec "$container_id" pg_isready --host 127.0.0.1 -U mira_test -d mira_test >/dev/null 2>&1; then break; fi
    sleep 1
  done
  port="$(docker port "$container_id" 5432/tcp | sed 's/.*://')"
  export TEST_POSTGRES_DATABASE_URL="postgresql://mira_test@127.0.0.1:${port}/mira_test"
fi
if [[ "$(psql "$TEST_POSTGRES_DATABASE_URL" -Atqc 'SELECT current_database()')" != "mira_test" ]]; then
  echo "Las pruebas requieren una base dedicada llamada mira_test." >&2
  exit 1
fi
psql "$TEST_POSTGRES_DATABASE_URL" -v ON_ERROR_STOP=1 -q <<'SQL'
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='anon') THEN CREATE ROLE anon; END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='authenticated') THEN CREATE ROLE authenticated; END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='service_role') THEN CREATE ROLE service_role; END IF;
END $$;
CREATE SCHEMA IF NOT EXISTS mira;
CREATE TABLE IF NOT EXISTS mira.schema_migrations (
  version TEXT PRIMARY KEY,
  checksum TEXT NOT NULL,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
REVOKE ALL ON mira.schema_migrations FROM PUBLIC, anon, authenticated, service_role;
SQL
for migration in "$repo_dir"/apps/api/migrations/*.sql; do
  psql "$TEST_POSTGRES_DATABASE_URL" -v ON_ERROR_STOP=1 -q -f "$migration"
  version="$(basename "$migration" .sql)"
  checksum="$(sha256sum "$migration" | cut -d' ' -f1)"
  psql "$TEST_POSTGRES_DATABASE_URL" -v ON_ERROR_STOP=1 -q -v version="$version" -v checksum="$checksum" <<'SQL'
INSERT INTO mira.schema_migrations(version,checksum)
VALUES (:'version',:'checksum') ON CONFLICT (version) DO NOTHING;
SQL
done
cargo test --manifest-path "$repo_dir/apps/api/Cargo.toml" postgres::tests:: -- --ignored

# El drill usa el script de respaldo real y una base nueva. Si ese nombre ya
# existe, CREATE falla y no se borra ninguna base ajena a esta ejecución.
psql "$TEST_POSTGRES_DATABASE_URL" -v ON_ERROR_STOP=1 -qc 'CREATE DATABASE mira_restore_test'
restore_created="1"
restore_url="$(python3 - <<'PY'
import os
from urllib.parse import urlsplit, urlunsplit
source = urlsplit(os.environ['TEST_POSTGRES_DATABASE_URL'])
print(urlunsplit(source._replace(path='/mira_restore_test')))
PY
)"
if [[ -n "$container_id" ]]; then
  # Los clientes del contenedor coinciden con la versión del servidor local.
  docker exec --env POSTGRES_DATABASE_URL=postgresql://mira_test@127.0.0.1/mira_test \
    "$container_id" bash /repo/scripts/backup-postgres.sh /backup
  dumps=("$drill_dir"/mira-*.dump)
  docker exec "$container_id" pg_restore --exit-on-error --no-owner --no-acl \
    --dbname=postgresql://mira_test@127.0.0.1/mira_restore_test "/backup/$(basename "${dumps[0]}")"
elif [[ -n "${TEST_POSTGRES_CONTAINER_ID:-}" ]]; then
  # GitHub Actions entrega el contenedor PostgreSQL del job; usa su cliente
  # para que un upgrade del runner no cambie el formato del respaldo.
  ci_backup_dir="$(docker exec "$TEST_POSTGRES_CONTAINER_ID" mktemp -d)"
  docker cp "$repo_dir/scripts/backup-postgres.sh" "$TEST_POSTGRES_CONTAINER_ID:$ci_backup_dir/backup.sh"
  docker exec --env POSTGRES_DATABASE_URL=postgresql://mira_test@127.0.0.1/mira_test \
    "$TEST_POSTGRES_CONTAINER_ID" bash "$ci_backup_dir/backup.sh" "$ci_backup_dir"
  docker cp "$TEST_POSTGRES_CONTAINER_ID:$ci_backup_dir/." "$drill_dir"
  dumps=("$drill_dir"/mira-*.dump)
  docker exec "$TEST_POSTGRES_CONTAINER_ID" pg_restore --exit-on-error --no-owner --no-acl \
    --dbname=postgresql://mira_test@127.0.0.1/mira_restore_test "$ci_backup_dir/$(basename "${dumps[0]}")"
else
  POSTGRES_DATABASE_URL="$TEST_POSTGRES_DATABASE_URL" \
    bash "$repo_dir/scripts/backup-postgres.sh" "$drill_dir"
  dumps=("$drill_dir"/mira-*.dump)
  pg_restore --exit-on-error --no-owner --no-acl --dbname="$restore_url" "${dumps[0]}"
fi

# Compara el contenido de todas las tablas sin imprimir filas, tokens ni DSN.
psql "$TEST_POSTGRES_DATABASE_URL" -Atqc "
SELECT format('SELECT %L,count(*),md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text)::text,''[]'')) FROM %I.%I t;',schemaname||'.'||tablename,schemaname,tablename)
FROM pg_tables WHERE schemaname IN ('mira','billing') ORDER BY schemaname,tablename;
" > "$drill_dir/compare.sql"
cat >> "$drill_dir/compare.sql" <<'SQL'
SELECT 'index',schemaname,indexname,indexdef FROM pg_indexes
WHERE schemaname IN ('mira','billing') ORDER BY schemaname,indexname;
SELECT 'column',table_schema,table_name,column_name,data_type,is_nullable,column_default
FROM information_schema.columns WHERE table_schema IN ('mira','billing')
ORDER BY table_schema,table_name,ordinal_position;
SELECT 'rls',n.nspname,c.relname,c.relrowsecurity
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname IN ('mira','billing') AND c.relkind='r' ORDER BY n.nspname,c.relname;
SELECT 'constraint',n.nspname,c.relname,k.conname,pg_get_constraintdef(k.oid)
FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname IN ('mira','billing') ORDER BY n.nspname,c.relname,k.conname;
SQL
psql "$TEST_POSTGRES_DATABASE_URL" -v ON_ERROR_STOP=1 -Atq -f "$drill_dir/compare.sql" > "$drill_dir/source.txt"
psql "$restore_url" -v ON_ERROR_STOP=1 -Atq -f "$drill_dir/compare.sql" > "$drill_dir/restored.txt"
if ! cmp -s "$drill_dir/source.txt" "$drill_dir/restored.txt"; then
  echo "El restore no conserva datos o estructura de mira/billing." >&2
  exit 1
fi
for role in anon authenticated service_role; do
  for relation in mira.records billing.subscriptions billing.checkout_sessions; do
    if psql "$restore_url" -v ON_ERROR_STOP=1 -qc "SET ROLE $role; SELECT 1 FROM $relation LIMIT 1" >/dev/null 2>&1; then
      echo "El restore permite acceso a un rol público." >&2
      exit 1
    fi
  done
done
echo "OK: backup/restore conserva todas las tablas, índices y RLS de mira y billing."
