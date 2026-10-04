#!/usr/bin/env bash
set -euo pipefail
umask 077

# Respaldo manual de la base PostgreSQL de Mira (Supabase, schemas `mira` y `billing`).
#
# Uso:
#   POSTGRES_DATABASE_URL='postgresql://...' scripts/backup-postgres.sh [directorio_destino]
#
# La URL es la misma que usa la API (Secret Manager: mira-postgres-url).
# No se registra ni se imprime. El destino queda fuera de Git (backups/ está
# en .gitignore).
#
# Rotación: conserva los últimos BACKUP_KEEP dumps (14 por defecto).
#
# Cron sugerido (DESACTIVADO por decisión 2026-07-18; activar cuando la
# persona responsable lo decida — requiere que la URL esté disponible en el
# entorno del cron sin quedar escrita en el crontab):
#   # 0 8 * * 1-5  cd /ruta/al/repo && POSTGRES_DATABASE_URL="$(comando_que_la_obtiene)" scripts/backup-postgres.sh
#
# Restauración: ver docs/operational-runbooks.md, sección «Respaldo y
# restauración PostgreSQL (Supabase)».

DEST="${1:-backups/postgres}"
KEEP="${BACKUP_KEEP:-14}"
: "${POSTGRES_DATABASE_URL:?POSTGRES_DATABASE_URL es obligatoria (no se lee de .env a propósito)}"
if [[ ! "$KEEP" =~ ^[1-9][0-9]{0,4}$ ]]; then
  echo "BACKUP_KEEP debe ser un entero entre 1 y 99999." >&2
  exit 1
fi

mkdir -p -- "$DEST"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
partial="$(mktemp "$DEST/mira-$stamp-XXXXXX.partial")"
trap 'rm -f -- "$partial"' EXIT
file="${partial%.partial}.dump"

# Formato custom: permite verificar con pg_restore --list y restaurar tablas
# sueltas. --no-owner/--no-acl para restaurar en cualquier rol destino.
pg_dump "$POSTGRES_DATABASE_URL" \
  --schema=mira \
  --schema=billing \
  --format=custom \
  --no-owner \
  --no-acl \
  --file "$partial"

# Verificación de integridad: un dump truncado o corrupto no pasa el listado.
dump_listing="$(pg_restore --list "$partial")"

# Antes de 0002, `billing` todavía no existe: ese respaldo sigue siendo válido.
tables=("mira records" "mira schema_migrations")
if grep -q "SCHEMA - billing" <<< "$dump_listing"; then
  tables+=("billing plans" "billing subscriptions" "billing checkout_sessions" "billing usage_ledger" "billing quota_alerts")
fi
for table in "${tables[@]}"; do
  if ! grep -q "TABLE DATA $table" <<< "$dump_listing"; then
    echo "ERROR: el dump no contiene ${table/ /.}" >&2
    exit 1
  fi
done
mv -- "$partial" "$file"

# Rotación: sólo después de un respaldo verificado, con nombres seguros incluso
# si el directorio contiene espacios. Los archivos incompletos no cuentan.
mapfile -d '' -t backups < <(
  find "$DEST" -maxdepth 1 -type f -name 'mira-*.dump' -printf '%T@ %p\0' |
    sort -zr | cut -z -d' ' -f2-
)
if (( ${#backups[@]} > KEEP )); then
  rm -- "${backups[@]:KEEP}"
fi

count="${#backups[@]}"
if (( count > KEEP )); then count="$KEEP"; fi
echo "OK: $file ($(du -h "$file" | cut -f1)); $count respaldo(s) conservado(s) en $DEST"
