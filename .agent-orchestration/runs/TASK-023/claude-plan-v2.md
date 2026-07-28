# TASK-023 — Historial de Operaciones (Beta Incremental)

## 1. Resumen ejecutivo

Agregar un historial corto de ejecuciones del scheduler visible en ConfiguracionView. Sin almacenar cuerpos de correo ni datos sensibles. Compatible con los stores existentes (MemoryStorage / FirestoreStorage) y el scope Gmail readonly.

---

## 2. Contrato API

### 2.1 Endpoint nuevo

```
GET /me/operations/history
```

**Query params opcionales:**

| Parámetro | Tipo | Default | Descripción |
|-----------|------|---------|-------------|
| `limit` | u8 | 10 | Máximo de entradas, tope 50 |
| `offset` | u64 | 0 | Paginación por cursor numérico |

**Response 200:**

```json
{
  "entries": [
    {
      "run_id": "run_20260617T143000Z",
      "started_at": "2026-06-17T14:30:00Z",
      "finished_at": "2026-06-17T14:30:47Z",
      "duration_ms": 47200,
      "status": "success",
      "emails_inspected": 34,
      "emails_actioned": 5,
      "error_category": null,
      "scheduler_preset": "morning_digest"
    },
    {
      "run_id": "run_20260616T143000Z",
      "started_at": "2026-06-16T14:30:00Z",
      "finished_at": "2026-06-16T14:30:12Z",
      "duration_ms": 12100,
      "status": "partial_failure",
      "emails_inspected": 12,
      "emails_actioned": 0,
      "error_category": "llm_timeout",
      "scheduler_preset": "morning_digest"
    }
  ],
  "total_count": 87,
  "has_more": true
}
```

**Response 404:** usuario no tiene historial aún → `{ "entries": [], "total_count": 0, "has_more": false }`

**No se expone:** mensaje de error crudo, thread_id, snippet, destinatarios individuales, body de correo.

---

## 3. Modelo de datos

### 3.1 Struct Rust — `OperationEntry`

```rust
pub struct OperationEntry {
    pub run_id: String,           // formato: run_{ISO8601_compact}
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub status: RunStatus,
    pub emails_inspected: u32,
    pub emails_actioned: u32,
    pub error_category: Option<ErrorCategory>,
    pub scheduler_preset: Option<String>,
}

pub enum RunStatus {
    Success,
    PartialFailure,
    Failure,
    InProgress,
}
```

### 3.2 Firestore path

```
operationHistory/{email}/runs/{run_id}
```

TTL de documento: 90 días. Se implementa con un campo `expires_at` + Cloud Firestore TTL policy (sin costo extra de código).

### 3.3 Memory path (tests / dev)

`HashMap<String, Vec<OperationEntry>>` keyed por email. Límite de 100 entradas en memoria; descarta la más antigua.

---

## 4. Categorías de error (no sensibles)

Enum exhaustivo, nunca expone stack traces ni mensajes de LLM crudos:

| Variante | Cuándo |
|----------|--------|
| `gmail_quota_exceeded` | 429 de la API de Gmail |
| `gmail_auth_expired` | token OAuth revocado o expirado |
| `llm_timeout` | bedrock/claude no responde en SLA |
| `llm_overloaded` | bedrock devuelve 429/503 |
| `classification_error` | LLM devuelve JSON malformado |
| `storage_write_error` | fallo al persistir resultado |
| `scheduler_overlap` | run anterior aún activo |
| `unknown` | cualquier otro error no clasificado |

La lógica de mapeo vive en un `fn categorize_error(e: &AppError) -> ErrorCategory` en el módulo `scheduler`, no en el handler HTTP.

---

## 5. Storage — cambios

### 5.1 Trait `OperationHistoryStore`

```rust
#[async_trait]
pub trait OperationHistoryStore: Send + Sync {
    async fn append_entry(&self, email: &str, entry: OperationEntry) -> Result<()>;
    async fn list_entries(
        &self,
        email: &str,
        limit: u8,
        offset: u64,
    ) -> Result<(Vec<OperationEntry>, u64)>;  // (entries, total_count)
}
```

### 5.2 `MemoryStorage` impl

- Guarda en `Arc<Mutex<HashMap<String, VecDeque<OperationEntry>>>>`.
- `append_entry` pushea al frente; si len > 100 popea el final.
- `list_entries` clona el slice pedido.

### 5.3 `FirestoreStorage` impl

- `append_entry` → `set_doc` en `operationHistory/{email}/runs/{run_id}`.
- `list_entries` → query ordenada por `started_at` DESC con `limit`/`offset`.
- No modifica ningún documento de Gmail ni `scheduleStates`.

### 5.4 Hook en el scheduler

En `scheduler/mod.rs`, al finalizar cada window:

```rust
let entry = OperationEntry { ... };
store.append_entry(&email, entry).await?;
```

Fallo de escritura → log warn, no interrumpe el flujo principal.

---

## 6. UI — ConfiguracionView

### 6.1 Nueva sección bajo "Operación automática"

```
┌─ Historial reciente ────────────────────────────────┐
│  17 Jun 14:30  ✓ 5 acciones / 34 revisados  47s    │
│  16 Jun 14:30  ⚠ 0 acciones  llm_timeout    12s    │
│  15 Jun 14:30  ✓ 3 acciones / 28 revisados  39s    │
│                           [ Ver más (87 total) ]    │
└─────────────────────────────────────────────────────┘
```

### 6.2 Tipo TypeScript nuevo en `api/types.ts`

```ts
export interface OperationEntry {
  run_id: string
  started_at: string
  finished_at: string | null
  duration_ms: number | null
  status: 'success' | 'partial_failure' | 'failure' | 'in_progress'
  emails_inspected: number
  emails_actioned: number
  error_category: string | null
  scheduler_preset: string | null
}

export interface OperationHistoryResponse {
  entries: OperationEntry[]
  total_count: number
  has_more: boolean
}
```

### 6.3 Fetch en `api/client.ts`

```ts
export async function getOperationHistory(limit = 10, offset = 0) {
  return apiFetch<OperationHistoryResponse>(
    `/me/operations/history?limit=${limit}&offset=${offset}`
  )
}
```

### 6.4 Estado local en ConfiguracionView

- `historyEntries`, `historyLoading`, `historyError`, `historyOffset`.
- Carga en mount si scheduler está habilitado.
- Botón "Ver más" incrementa offset y concatena resultados.
- Estado vacío: "Sin ejecuciones aún."
- Estado error: "No se pudo cargar el historial." (sin exponer error_category al usuario en texto plano; sólo el badge de color).

### 6.5 Badge de estado

| status | color token | icono |
|--------|-------------|-------|
| success | `--color-success` | ✓ |
| partial_failure | `--color-warning` | ⚠ |
| failure | `--color-error` | ✗ |
| in_progress | `--color-info` | ↻ |

---

## 7. Tests

### 7.1 Rust — unitarios

- `categorize_error` mapea cada variante de `AppError` al `ErrorCategory` correcto.
- `MemoryStorage::append_entry` hace ring-buffer a 100 entradas.
- `MemoryStorage::list_entries` respeta limit/offset y devuelve total_count correcto.

### 7.2 Rust — integración

- `GET /me/operations/history` sin historial → `{ entries: [], total_count: 0, has_more: false }`.
- Con 3 entradas en MemoryStorage → devuelve las 3 ordenadas desc.
- `limit=2&offset=0` → `has_more: true`.
- Sin auth header → 401.

### 7.3 TypeScript — componente

- Renderiza "Sin ejecuciones aún" cuando `entries` es vacío.
- Renderiza badge correcto para cada status.
- "Ver más" dispara segunda llamada con offset=10.
- Error de red muestra mensaje de fallback.

---

## 8. Criterios de aceptación

- [ ] `GET /me/operations/history` devuelve hasta 50 entradas ordenadas por `started_at` DESC.
- [ ] Ningún campo de respuesta contiene cuerpo de correo, snippet, thread_id, ni stack trace.
- [ ] `error_category` es uno de los 8 valores del enum o `null`.
- [ ] Cada ejecución del scheduler escribe una entrada; fallo de escritura no rompe el scheduler.
- [ ] TTL de 90 días configurado en Firestore; MemoryStorage limita a 100 entradas.
- [ ] UI muestra historial con badge de estado y duración; carga paginada funciona.
- [ ] Cobertura de tests ≥ 80% en código nuevo.
- [ ] `GET /me/operations/status` existente no cambia su contrato.

---

## 9. Riesgos y mitigaciones

| Riesgo | Probabilidad | Mitigación |
|--------|-------------|------------|
| Firestore TTL no activa en dev | Media | MemoryStorage en dev; TTL sólo en staging/prod |
| Scheduler corre concurrentemente y duplica entries | Baja | `run_id` como doc ID → escritura idempotente |
| Offset paginación lento en Firestore (no hay cursor real) | Media | Limitar total a 500 runs guardados por usuario; offset numérico aceptable para beta |
| UI muestra `error_category` crudo y confunde al usuario | Baja | Mapear en frontend a mensajes amigables antes de render |
| Scope Gmail readonly | N/A — ya garantizado | El scheduler no escribe correos; este feature sólo lee metadata de runs propios |

---

## 10. Orden de implementación

1. Enum `ErrorCategory` + `categorize_error` (Rust, sin dependencias externas)
2. `OperationEntry` struct + `RunStatus` enum
3. Trait `OperationHistoryStore` + impl `MemoryStorage`
4. Tests unitarios de storage
5. Hook en scheduler (`append_entry` al finalizar cada window)
6. Handler HTTP `GET /me/operations/history` + route registration
7. Tests de integración del endpoint
8. Impl `FirestoreStorage` (puede ir en paralelo con 6-7)
9. Tipos TS + `getOperationHistory` en client
10. UI en ConfiguracionView + tests de componente
11. QA manual: verificar que ningún campo sensible llega al frontend
