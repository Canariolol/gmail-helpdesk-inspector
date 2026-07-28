# Human-in-the-loop protocol

Objetivo: mantener al dueño del producto dentro del ciclo de decisión sin bloquear tareas mecánicas.

## Regla central

Los agentes no deben resolver ambigüedades de producto, seguridad, privacidad, modelo de negocio o datos sensibles inventando supuestos. Deben elevarlas a Pi. Pi debe consolidar contexto, proponer opciones y preguntar al dueño del producto cuando la decisión afecte dirección, riesgo o experiencia pública.

## Cómo deben preguntar los agentes

En modo no interactivo, Codex/Claude deben incluir al inicio o final de su output un bloque:

```md
## BLOCKED_QUESTIONS

1. Pregunta concreta.
   - Opción A: ...
   - Opción B: ...
   - Recomendación del agente: ...
   - Impacto si se decide mal: ...
```

Si la pregunta bloquea la tarea, deben detenerse y no implementar. Si no bloquea, deben declarar el supuesto usado:

```md
## ASSUMPTIONS

- Supuse X para poder avanzar; requiere confirmación de Pi/owner antes de merge.
```

## Qué puede decidir Pi sin preguntar

- Refactors internos reversibles dentro del alcance aprobado.
- Nombres de archivos/componentes no públicos.
- Orden de implementación cuando no cambia producto ni seguridad.
- Ejecutar tests/checks.
- Rechazar outputs inseguros o fuera de alcance.

## Qué debe preguntar Pi al dueño del producto

- Público objetivo, pricing, beta privada vs pública, invitaciones.
- Retención de datos, borrado, exportación, IA opcional/obligatoria.
- Cambios en permisos/scopes Google.
- Persistencia de cuerpos completos de email.
- Textos legales, privacidad, términos, subprocessors.
- Decisiones de UX que prometan capacidades todavía no implementadas.
- Cualquier tradeoff entre rapidez y seguridad.
- Cualquier hardcode heredado de la empresa original que deba generalizarse.

## Decision gates

### Gate A — Plan

Antes de implementar una tarea sensible debe existir plan aprobado en `runs/TASK-XXX/`.

### Gate B — Implementación

Antes de editar código, la task debe declarar paths permitidos y criterios de aceptación.

### Gate C — Merge local

Antes de considerar listo:

- tests/checks relevantes ejecutados o justificación de por qué no;
- riesgos documentados;
- decisiones nuevas registradas en `decisions.md`;
- no secretos leídos ni modificados.

## Política sobre hardcodes del cliente original

El sistema nació para una persona/empresa concreta. Para SaaS, cualquier valor específico de empresa, dominio, contexto del auditor IA, timezone, destinatarios de reportes, umbrales o filtros debe migrar a configuración por usuario/organización o a defaults seguros editables.

No basta con más checkboxes. La configuración debe modelar intención operacional: quién es interno, qué tipo de solicitudes cuentan, qué fuentes se ignoran, cómo audita la IA, qué datos se retienen y qué evidencia necesita el usuario.
