# Path ownership y zonas protegidas

## Zonas bloqueadas para agentes externos

Nunca leer, copiar, resumir ni modificar:

- `.env`
- `.env.*` con valores reales
- `secrets/*`
- claves GCP, tokens OAuth, API keys, refresh tokens
- archivos temporales con credenciales

Permitido leer/modificar solo plantillas seguras:

- `.env.example`
- documentación sin secretos

## Ownership por área

| Área | Paths | Owner primario | Review |
|---|---|---|---|
| Orquestación | `.agent-orchestration/**`, `scripts/agents/**` | Pi | Codex si afecta protocolo |
| Frontend UI/UX | `apps/web/src/**`, `apps/web/index.html`, CSS | Claude | Pi + Codex si cambia contratos |
| API/backend | `apps/api/src/**`, `apps/api/Cargo.toml` | Codex | Pi |
| AI worker | `apps/ai-worker/src/**`, `apps/ai-worker/pyproject.toml` | Codex | Pi + Claude si afecta UX copy |
| Docs producto/privacidad | `README.md`, `docs/**`, `specs_prd.md` | Pi/Codex | Claude para UX-facing docs |
| Infra local/deploy | `docker-compose.yml`, `Dockerfile`, `scripts/*.sh` | Codex | Pi |

## Reglas de edición

- Una task debe listar paths permitidos y prohibidos.
- Si un agente descubre que necesita tocar otro path, debe pedir handoff en su output; no debe hacerlo directamente.
- Cambios contractuales API/frontend requieren:
  1. Codex planifica contrato.
  2. Pi aprueba.
  3. Claude implementa UI contra contrato.
  4. Codex/Pi revisan integración.

## Hotspots que requieren cuidado extra

- OAuth Google y refresh tokens.
- Encriptación de tokens.
- Scope Gmail readonly.
- Firestore auth y service account.
- Scheduler y envío de reportes por Resend.
- Cualquier logging de snippets/cuerpos de correo.
- Cálculo de métricas y trazabilidad.
