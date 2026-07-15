#!/usr/bin/env bash
set -euo pipefail

if git grep -nIE \
  -e '-----BEGIN [A-Z ]*PRIVATE KEY-----' \
  -e 'AKIA[0-9A-Z]{16}' \
  -e 'AIza[0-9A-Za-z_-]{35}' \
  -e 'GOCSPX-[0-9A-Za-z_-]{20,}' \
  -e 'APP_USR-[0-9A-Za-z_-]{20,}' \
  -e 'sk_(live|prod)_[0-9A-Za-z_-]{16,}' \
  -e 'ghp_[0-9A-Za-z]{36}' \
  -e 'github_pat_[0-9A-Za-z_]{20,}'
then
  echo "Potential credential found in a tracked file." >&2
  exit 1
fi
