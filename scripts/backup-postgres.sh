#!/usr/bin/env bash
set -euo pipefail

# Respaldo manual de la base PostgreSQL de Mira (Supabase, schema `mira`).
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

mkdir -p "$DEST"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
file="$DEST/mira-$stamp.dump"

# Formato custom: permite verificar con pg_restore --list y restaurar tablas
# sueltas. --no-owner/--no-acl para restaurar en cualquier rol destino.
pg_dump "$POSTGRES_DATABASE_URL" \
  --schema=mira \
  --format=custom \
  --no-owner \
  --no-acl \
  --file "$file"

# Verificación de integridad: un dump truncado o corrupto no pasa el listado.
pg_restore --list "$file" >/dev/null

# El dump debe contener las dos tablas conocidas del schema.
for table in records schema_migrations; do
  if ! pg_restore --list "$file" | grep -q "TABLE DATA mira $table"; then
    echo "ERROR: el dump no contiene mira.$table" >&2
    exit 1
  fi
done

# Rotación: conserva los KEEP más recientes.
ls -1t "$DEST"/mira-*.dump 2>/dev/null | tail -n +"$((KEEP + 1))" | xargs -r rm --

count="$(ls -1 "$DEST"/mira-*.dump | wc -l)"
echo "OK: $file ($(du -h "$file" | cut -f1)); $count respaldo(s) conservado(s) en $DEST"
