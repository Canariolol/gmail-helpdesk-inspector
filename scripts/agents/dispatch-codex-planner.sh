#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:?Usage: $0 TASK-XXX}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TASK_FILE="$ROOT/.agent-orchestration/tasks/$TASK_ID.md"
RUN_DIR="$ROOT/.agent-orchestration/runs/$TASK_ID"
OUT="$RUN_DIR/codex-plan.md"
LOG="$RUN_DIR/codex-plan.log"
MODEL="${CODEX_PLANNER_MODEL:-gpt-5.5}"
EFFORT="${CODEX_REASONING_EFFORT:-high}"
SANDBOX="${CODEX_SANDBOX:-read-only}"

if [[ ! -f "$TASK_FILE" ]]; then
  echo "Task file not found: $TASK_FILE" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"
PROMPT_FILE="$(mktemp)"
trap 'rm -f "$PROMPT_FILE"' EXIT

cat > "$PROMPT_FILE" <<EOF_PROMPT
Eres Codex actuando como planner principal, arquitecto y reviewer de seguridad para este repo.

Reglas obligatorias:
- Lee y respeta .agent-orchestration/roles.md, .agent-orchestration/path-ownership.md y .agent-orchestration/human-in-loop.md.
- No leas .env, .env.* reales, secrets/*, tokens ni credenciales.
- Esta ejecución es read-only salvo que la task diga explícitamente lo contrario.
- No modifiques código. Produce únicamente el output solicitado.
- Si necesitas tocar paths no permitidos, decláralo como handoff; no lo hagas.
- Ante ambigüedades de producto, seguridad, privacidad, SaaS, datos o UX contractual, no inventes supuestos: incluye BLOCKED_QUESTIONS con opciones, recomendación e impacto. Si puedes avanzar sin riesgo, declara ASSUMPTIONS.
- Prioriza calidad, seguridad, privacidad, trazabilidad y launch readiness.

Task:

$(cat "$TASK_FILE")
EOF_PROMPT

CMD=(codex exec -C "$ROOT" -s "$SANDBOX" -m "$MODEL" -c "model_reasoning_effort=\"$EFFORT\"" -o "$OUT")
if [[ -n "${CODEX_PLANNER_PROFILE:-}" ]]; then
  CMD+=(-p "$CODEX_PLANNER_PROFILE")
fi

{
  echo "# $(date -Is)"
  echo "# Command: ${CMD[*]} - <prompt>"
  echo
} > "$LOG"

"${CMD[@]}" - < "$PROMPT_FILE" 2>&1 | tee -a "$LOG"

echo "Saved Codex output to: $OUT"
