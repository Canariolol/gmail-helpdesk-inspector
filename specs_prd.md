# Gmail Helpdesk Metrics Inspector — SPEC / PRD

## 1. Resumen

Sistema web open source para analizar una casilla de Gmail usada como help desk y generar métricas operativas básicas: correos recibidos, correos válidos, correos respondidos, correos no respondidos, tiempos de primera respuesta y casos ambiguos.

El sistema debe funcionar bajo demanda: el usuario inicia sesión con Google, autoriza acceso de lectura a Gmail, configura criterios mínimos de análisis y presiona **Analizar**.

El sistema no busca ser una plataforma de tickets completa. Su objetivo es entregar métricas confiables y auditables sobre una casilla Gmail existente.

---

## 2. Objetivos

### Objetivos principales

* Analizar correos de una casilla Gmail autenticada.
* Identificar qué hilos corresponden a solicitudes válidas de clientes.
* Excluir correos internos, automáticos, newsletters, avisos de Google, spam y misceláneos.
* Determinar qué solicitudes fueron respondidas.
* Calcular tiempo de primera respuesta.
* Mostrar métricas agregadas en un dashboard.
* Permitir revisión manual de casos ambiguos o mal clasificados.
* Usar IA como apoyo de auditoría y clasificación, no como fuente principal de conteo.
* Mantener trazabilidad entre cada métrica y los hilos que la componen.

### No objetivos

* No será un sistema de tickets.
* No enviará correos.
* No modificará mensajes de Gmail.
* No asignará responsables.
* No calculará SLA contractual complejo en la primera versión.
* No requiere multiempresa ni modelo SaaS inicialmente.
* No requiere sincronización permanente en background para el MVP.

---

## 3. Stack propuesto

### Frontend

* React
* TypeScript
* Vite
* TanStack Query o equivalente para estado de requests
* TailwindCSS opcional
* Chart library simple, por ejemplo Recharts

### Backend principal

* Rust
* Axum recomendado
* OAuth Google
* Gmail API
* SQLite para MVP
* SQLx o SeaORM

### Worker IA opcional

* Python
* FastAPI o script worker simple
* Usado solo para:

  * clasificación de casos ambiguos
  * auditoría de muestra
  * detección de posibles errores en el reporte

### Base de datos

MVP:

* SQLite

Futuro:

* Postgres, si el sistema crece o requiere multiusuario real.

---

## 4. Flujo de usuario

```text
1. Usuario entra a la web.
2. Presiona "Login with Google".
3. Autoriza permiso Gmail readonly.
4. El sistema muestra pantalla de configuración.
5. Usuario define:
   - rango de fechas
   - dominio interno de la empresa
   - remitentes/dominios ignorados
   - palabras clave ignoradas
   - si desea usar auditoría IA
6. Usuario presiona "Analizar".
7. Backend obtiene hilos desde Gmail.
8. Sistema clasifica hilos y calcula métricas.
9. IA revisa casos ambiguos/sospechosos si está activada.
10. Usuario ve dashboard.
11. Usuario puede revisar manualmente hilos ambiguos.
12. Las correcciones manuales actualizan el reporte.
```

---

## 5. Permisos Google

Usar el scope mínimo:

```text
https://www.googleapis.com/auth/gmail.readonly
```

El sistema solo debe leer correos. No debe enviar, borrar, etiquetar ni modificar mensajes.

---

## 6. Entidades principales

### UserSession

Representa una sesión autenticada con Google.

Campos sugeridos:

```text
id
google_account_email
access_token_encrypted
refresh_token_encrypted
created_at
updated_at
```

Para MVP local, el almacenamiento de tokens puede simplificarse, pero debe quedar aislado y nunca commiteado.

---

### AnalysisRun

Representa una ejecución de análisis.

```text
id
user_email
date_from
date_to
internal_domains
ignored_senders
ignored_domains
ignored_keywords
use_ai_audit
status
created_at
completed_at
error_message
```

Estados:

```text
pending
running
completed
failed
```

---

### EmailThread

Representa un hilo Gmail analizado.

```text
id
analysis_run_id
gmail_thread_id
subject
normalized_subject
classification
classification_source
classification_confidence
is_valid_client_request
is_answered
first_client_message_id
first_internal_reply_message_id
first_client_message_at
first_internal_reply_at
response_time_minutes
manual_review_required
manual_override_applied
created_at
updated_at
```

Clasificaciones posibles:

```text
valid_client_request
internal
automated
newsletter
spam
misc
ambiguous
```

Fuentes de clasificación:

```text
rules
heuristics
ai
manual
```

---

### EmailMessage

Representa un mensaje dentro de un hilo.

```text
id
email_thread_id
gmail_message_id
from_email
from_name
to_emails
cc_emails
date
subject
snippet
headers_json
is_internal
is_external
is_automated
created_at
```

---

### ManualReview

Representa una corrección humana.

```text
id
email_thread_id
reviewer_label
new_classification
is_valid_client_request
is_answered
first_client_message_id
first_internal_reply_message_id
notes
created_at
```

---

## 7. Lógica de clasificación

La clasificación debe realizarse por capas.

### Capa 1: reglas duras

Excluir automáticamente:

* Hilos donde todos los participantes son internos.
* Correos desde `noreply`, `no-reply`, `notification`, `mailer-daemon`.
* Correos de Google, calendarios, alertas automáticas o sistemas conocidos.
* Newsletters detectadas por headers o palabras clave.
* Correos sin señal de solicitud humana.

Ejemplos:

```text
from contains no-reply@ → automated
all senders internal → internal
subject contains "newsletter" → newsletter
from domain in ignored_domains → misc/ignored
```

---

### Capa 2: heurísticas

Marcar como solicitud válida si:

* El primer mensaje relevante viene de un dominio externo.
* El remitente no parece automático.
* El asunto o contenido parece una consulta, requerimiento, reclamo, solicitud o seguimiento.
* Hay destinatarios internos en la casilla help desk.

Marcar como respondido si:

* Existe un mensaje posterior al primer mensaje externo válido.
* El mensaje posterior viene desde un dominio interno.
* La respuesta no es automática.

Calcular primera respuesta:

```text
response_time = first_internal_reply_at - first_client_message_at
```

---

### Capa 3: casos sospechosos

Marcar para revisión IA o manual si:

* El hilo tiene muchos mensajes.
* Hay múltiples dominios externos.
* El primer mensaje parece automático pero luego participa una persona.
* El subject cambia demasiado.
* Hay forwards o respuestas reenviadas.
* Hay adjuntos pero poco texto.
* Hay mezcla de clientes/proveedores/internos.
* La respuesta interna ocurre antes del primer mensaje externo detectado.
* No hay claridad sobre si el remitente externo es cliente o proveedor.
* El hilo fue clasificado con baja confianza.

---

## 8. Uso de IA

La IA no debe contar correos directamente.

La IA debe actuar como:

```text
auditor
clasificador auxiliar
detector de inconsistencias
apoyo para casos ambiguos
```

### Entrada sugerida para IA

Enviar la mínima información necesaria:

```json
{
  "thread_id": "gmail-thread-id",
  "subject": "Asunto",
  "messages": [
    {
      "message_id": "id",
      "from": "cliente@example.com",
      "to": ["helpdesk@empresa.cl"],
      "date": "2026-06-12T10:20:00Z",
      "snippet": "Texto parcial..."
    }
  ],
  "automatic_classification": {
    "classification": "ambiguous",
    "is_valid_client_request": null,
    "is_answered": null
  }
}
```

### Salida esperada de IA

```json
{
  "classification": "valid_client_request",
  "is_valid_client_request": true,
  "is_answered": true,
  "first_client_message_id": "msg_1",
  "first_internal_reply_message_id": "msg_3",
  "confidence": 0.91,
  "manual_review_required": false,
  "issues": []
}
```

### Reglas para IA

* Si no hay evidencia suficiente, debe responder `ambiguous`.
* No debe inventar mensajes.
* No debe inferir clientes sin evidencia.
* No debe modificar métricas finales sin trazabilidad.
* Toda decisión IA debe quedar visible y revisable.

---

## 9. Revisión manual

La herramienta debe incluir revisión manual dentro del dashboard.

### Funcionalidades

* Listar hilos ambiguos.
* Ver mensajes del hilo en orden cronológico.
* Ver clasificación automática.
* Ver explicación de reglas/IA.
* Cambiar clasificación.
* Marcar como:

  * válido
  * ignorado
  * interno
  * automático
  * ambiguo
* Marcar como respondido/no respondido.
* Seleccionar primer mensaje cliente.
* Seleccionar primera respuesta interna.
* Agregar nota manual.
* Recalcular métricas tras aplicar correcciones.

### Principio clave

Toda métrica agregada debe permitir navegar a los hilos que la componen.

Ejemplo:

```text
Respondidos: 731
→ click
→ lista de 731 hilos
→ cada hilo muestra razón de clasificación
```

---

## 10. Dashboard MVP

Métricas principales:

```text
Total de hilos analizados
Solicitudes válidas
Solicitudes respondidas
Solicitudes no respondidas
Tiempo promedio de primera respuesta
Mediana de primera respuesta
P90 de primera respuesta
Hilos ignorados
Hilos ambiguos
Hilos corregidos manualmente
Confianza estimada del reporte
```

Visualizaciones:

* Cards de métricas principales.
* Gráfico por día/semana.
* Distribución de tiempos de respuesta.
* Tabla de hilos.
* Filtros por clasificación.
* Filtros por respondido/no respondido.
* Filtros por revisión manual pendiente.

---

## 11. Confianza del reporte

El reporte debe mostrar una confianza estimada.

Ejemplo:

```text
Confianza alta: 93%
Ambiguos: 38
Revisión manual pendiente: 12
Posibles falsos positivos detectados por IA: 7
Posibles falsos negativos detectados por IA: 4
```

La confianza puede calcularse inicialmente de forma simple:

```text
confidence = 1 - (ambiguous_threads + suspicious_threads) / total_candidate_threads
```

Luego puede mejorarse usando:

* confianza IA
* cantidad de overrides manuales
* tasa de errores detectados en muestra auditada
* proporción de hilos sospechosos

---

## 12. API interna sugerida

### Auth

```text
GET  /auth/google/login
GET  /auth/google/callback
POST /auth/logout
GET  /auth/me
```

### Analysis

```text
POST /analysis-runs
GET  /analysis-runs
GET  /analysis-runs/:id
POST /analysis-runs/:id/start
GET  /analysis-runs/:id/status
GET  /analysis-runs/:id/metrics
```

### Threads

```text
GET /analysis-runs/:id/threads
GET /threads/:thread_id
PATCH /threads/:thread_id/manual-review
```

### AI audit

```text
POST /analysis-runs/:id/ai-audit
POST /threads/:thread_id/ai-review
```

---

## 13. Componentes frontend

### Páginas

```text
LoginPage
SetupAnalysisPage
AnalysisProgressPage
DashboardPage
ThreadListPage
ThreadDetailPage
ManualReviewPage
SettingsPage
```

### Componentes

```text
MetricCard
DateRangePicker
DomainConfigForm
IgnoredSendersForm
AnalysisRunButton
AnalysisStatusBanner
ThreadTable
ThreadTimeline
ClassificationBadge
ConfidenceBadge
ManualReviewPanel
AiAuditPanel
MetricsChart
```

---

## 14. Estrategia MVP

### Fase 1 — Base funcional sin IA

* Login con Google.
* Lectura Gmail readonly.
* Selección de rango de fechas.
* Configuración de dominio interno.
* Obtención de threads.
* Clasificación por reglas.
* Cálculo de métricas.
* Dashboard básico.
* Tabla auditable de hilos.

### Fase 2 — Revisión manual

* Vista detalle de hilo.
* Corrección manual de clasificación.
* Corrección manual de respondido/no respondido.
* Selección manual de primer mensaje cliente.
* Selección manual de primera respuesta interna.
* Recalcular métricas.

### Fase 3 — Auditoría IA

* Worker Python.
* Revisión de hilos ambiguos.
* Revisión de muestra aleatoria.
* Detección de inconsistencias.
* Explicación de clasificación.
* Score de confianza.

### Fase 4 — Mejoras

* Export CSV.
* Export JSON.
* Guardado histórico de análisis.
* Configuraciones reutilizables.
* Soporte para horario laboral.
* Métricas por día hábil.
* Métricas por dominio cliente.
* Comparación entre períodos.

---

## 15. Consideraciones de privacidad

* Usar solo `gmail.readonly`.
* No guardar cuerpos completos salvo que sea estrictamente necesario.
* Preferir guardar metadata, headers y snippets.
* Permitir borrar análisis.
* No commitear tokens.
* Encriptar tokens si se persisten.
* Documentar claramente qué datos se leen.
* Para modo open source, incluir `.env.example`.
* Agregar advertencia sobre uso de datos sensibles.

---

## 16. Criterios de aceptación MVP

El MVP se considera funcional si:

* El usuario puede iniciar sesión con Google.
* El usuario puede analizar su propia casilla Gmail.
* El sistema obtiene hilos de un rango definido.
* El sistema identifica correos internos vs externos.
* El sistema excluye correos claramente automáticos.
* El sistema calcula:

  * total de hilos
  * válidos
  * respondidos
  * no respondidos
  * tiempo promedio de respuesta
  * mediana
* El usuario puede abrir la lista de hilos detrás de una métrica.
* El usuario puede corregir manualmente una clasificación.
* Las métricas se actualizan tras corrección manual.
* El sistema muestra hilos ambiguos.
* El sistema no modifica Gmail.

---

## 17. Riesgos

### Riesgo: Gmail agrupa mal algunos hilos

Mitigación:

* Usar `threadId` como base.
* Guardar `Message-ID`, `In-Reply-To`, `References` y subject normalizado.
* Marcar casos raros como sospechosos.

### Riesgo: clasificación incorrecta de clientes

Mitigación:

* Configuración de dominios internos.
* Lista de exclusiones.
* Revisión manual.
* Auditoría IA para ambiguos.

### Riesgo: conteos poco confiables

Mitigación:

* Trazabilidad total.
* Reporte con confianza estimada.
* Mostrar hilos incluidos/excluidos.
* Permitir override manual.

### Riesgo: IA inventa o sobreinterpreta

Mitigación:

* IA solo como apoyo.
* Respuestas JSON estrictas.
* Si no hay evidencia, marcar ambiguous.
* Mantener decisión determinística/manual como fuente final.

---

## 18. Nombre tentativo del proyecto

Opciones:

```text
gmail-helpdesk-inspector
maildesk-metrics
inbox-response-meter
support-mail-lens
```

Nombre recomendado:

```text
gmail-helpdesk-inspector
```

Es literal, claro y fácil de entender para un repo open source.

---

## 19. Licencia sugerida

Para un proyecto libre y abierto:

```text
MIT
```

Alternativas:

```text
Apache-2.0
GPL-3.0
```

Recomendación:

```text
MIT
```

Por simpleza, adopción y baja fricción.

---

## 20. Estructura inicial del repo

```text
gmail-helpdesk-inspector/
  README.md
  LICENSE
  SPEC.md
  .env.example
  docker-compose.yml

  apps/
    web/
      package.json
      vite.config.ts
      src/
        main.tsx
        App.tsx
        pages/
        components/
        api/

    api/
      Cargo.toml
      src/
        main.rs
        auth/
        gmail/
        analysis/
        threads/
        metrics/
        db/

    ai-worker/
      pyproject.toml
      src/
        main.py
        classify_thread.py
        audit_report.py

  docs/
    architecture.md
    privacy.md
    gmail-api-notes.md
```

---

## 21. Decisión técnica central

El sistema debe tener dos fuentes de verdad:

```text
1. Datos Gmail crudos normalizados.
2. Decisiones de clasificación auditables.
```

Las métricas nunca deben ser números mágicos. Cada métrica debe poder explicarse con los hilos y mensajes que la originaron.

Ese es el punto que hará que el sistema sea confiable.
