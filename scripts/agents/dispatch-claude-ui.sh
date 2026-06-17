#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:?Usage: $0 TASK-XXX}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TASK_FILE="$ROOT/.agent-orchestration/tasks/$TASK_ID.md"
RUN_DIR="$ROOT/.agent-orchestration/runs/$TASK_ID"
OUT="$RUN_DIR/claude-ui.md"
LOG="$RUN_DIR/claude-ui.log"
MODEL="${CLAUDE_UI_MODEL:-sonnet}"
EFFORT="${CLAUDE_UI_EFFORT:-max}"
PERMISSION_MODE="${CLAUDE_PERMISSION_MODE:-dontAsk}"
ALLOWED_TOOLS="${CLAUDE_ALLOWED_TOOLS:-Read,Grep,Glob,LS}"

if [[ ! -f "$TASK_FILE" ]]; then
  echo "Task file not found: $TASK_FILE" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"
PROMPT_FILE="$(mktemp)"
trap 'rm -f "$PROMPT_FILE"' EXIT

cat > "$PROMPT_FILE" <<EOF_PROMPT
Eres Claude actuando como especialista UI/UX y diseño de producto para este repo.

Reglas obligatorias:
- Lee y respeta .agent-orchestration/roles.md y .agent-orchestration/path-ownership.md.
- No leas .env, .env.* reales, secrets/*, tokens ni credenciales.
- Esta ejecución es de diseño/read-only. No modifiques código.
- No cambies contratos API/backend; declara dependencias como handoff.
- Prioriza confianza, privacidad, claridad, accesibilidad, trazabilidad y calidad visual.
- Si el resultado amerita Opus en vez de Sonnet, dilo explícitamente y justifica por qué.

Task:

$(cat "$TASK_FILE")
EOF_PROMPT

{
  echo "# $(date -Is)"
  echo "# Command: claude -p --model $MODEL --effort $EFFORT --permission-mode $PERMISSION_MODE --allowedTools $ALLOWED_TOOLS <prompt>"
  echo
} > "$LOG"

claude -p \
  --model "$MODEL" \
  --effort "$EFFORT" \
  --permission-mode "$PERMISSION_MODE" \
  --allowedTools "$ALLOWED_TOOLS" \
  -- \
  "$(cat "$PROMPT_FILE")" 2>&1 | tee -a "$LOG" | tee "$OUT" >/dev/null

echo "Saved Claude output to: $OUT"
