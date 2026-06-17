# SaaS Roadmap Consolidado

Fuente: TASK-001 (Codex), TASK-002 (Claude), TASK-003 (Codex).  
Fecha: 2026-06-17.

## Prioridad absoluta

No lanzar como SaaS público hasta resolver ownership/autorización, privacidad, revocación/borrado de datos y verificación OAuth de Google para `gmail.readonly`.

## P0 — Seguridad antes de cualquier beta amplia

1. Authz/ownership en endpoints de runs, metrics, threads, events y manual review.
2. Tests IDOR con dos usuarios.
3. `reviewer_label` controlado por backend, no por cliente.
4. Respuestas `404` para recursos ajenos con IDs opacos.
5. Fail-fast de secretos default en producción.
6. Redacción de errores públicos y logs sensibles.
7. CSRF/rate limiting para mutaciones y análisis.

## P1 — Confianza y privacidad visible

1. Login profesional con explicación de `gmail.readonly`.
2. Modal previo a OAuth explicando permisos.
3. Copy claro: no enviar, modificar, etiquetar ni borrar Gmail.
4. Sección de privacidad/datos dentro de la app.
5. Desconectar Gmail y revocar OAuth.
6. Borrar análisis/cuenta/datos.
7. Consentimiento explícito de auditoría IA.

## P2 — Arquitectura beta privada

1. Modelo mínimo de tenant/org/roles.
2. Firestore tenant-scoped o owner-scoped por defecto.
3. Configuración por usuario/tenant: dominios internos, exclusiones, IA, reportes.
4. Job runner persistente para análisis y scheduler.
5. Observabilidad por run/tenant/coste IA.
6. Paginación real en runs/threads/messages.

## P3 — Beta pública y SaaS

1. Google OAuth verification para scope restringido `gmail.readonly`.
2. Privacy Policy, Terms, DPA y subprocessors.
3. Retención configurable.
4. Límites de uso: threads/run, runs/día, reportes, usuarios.
5. Billing solo después de seguridad + OAuth verification.
6. Backup/restore, incident response y vulnerability disclosure.

## Diferenciación producto

- Métricas auditables: cada número navega a los hilos que lo componen.
- IA como auditor, no fuente primaria de conteo.
- Revisión manual guiada con impacto claro en métricas.
- Confianza del reporte como métrica visible.
- No compite con ticketing: se posiciona como auditor de Gmail helpdesk existente.

## Secuencia inmediata recomendada

1. Implementar TASK-003: authz/ownership P0.
2. Implementar TASK-004: quick wins UI de confianza que no dependen de backend.
3. Crear TASK para disconnect/delete data.
4. Crear TASK para configuración por usuario y remover hardcodes de empresa.
5. Preparar paquete OAuth verification + privacy docs.
