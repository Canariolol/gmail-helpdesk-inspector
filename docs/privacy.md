# Privacy — borrador técnico

Este documento describe el comportamiento técnico actual de Mira Helpdesk. No
es todavía una política de privacidad publicada: faltan responsable, contacto,
retención y revisión legal.

## Gmail Access

The only OAuth scope used is:

```text
https://www.googleapis.com/auth/gmail.readonly
```

The API does not call Gmail endpoints that send, delete, label, archive, or
modify messages.

## Stored Data

Firestore stores normalized metadata only:

- thread and message ids
- dates
- participants
- subject and snippets
- `Auto-Submitted` cuando existe, para detectar mensajes automáticos
- classification decisions and reasons
- audit findings
- aggregate metrics and AI token usage
- per-invocation operational metadata (run/attempt ids, token counts, outcome,
  stop reason and provider request id), never prompts or model responses

Los cuerpos se obtienen temporalmente durante el análisis. Para auditoría IA se
envían participantes, fecha, asunto y texto limitado por mensaje; un mensaje
corto puede caber completo dentro de ese límite. Luego se descartan tras la
respuesta de auditoría y no se persisten como cuerpos completos.

## AI Audit

The Bedrock worker receives text-only thread content. Attachments, images,
base64 payloads, and multimodal artifacts are intentionally excluded.
Model Invocation Logging remains disabled because it would persist prompts and
responses in AWS. Mira records only the metadata needed to reconcile usage and
diagnose failures.

AI auditing is enabled by default for a new organization. It can be disabled
from Configuration; when it is disabled, ambiguous threads remain available for
manual review. Re-enabling it requires explicit confirmation and applies to
future analyses.
