# TASK-024: Paginación básica beta — Plan de implementación

---

## 1. Objetivo

Añadir paginación cursor-based a `GET /analysis-runs` y `GET /analysis-runs/:id/threads` sin romper el frontend actual. La UI sigue funcionando con arrays directos; la paginación es opt-in vía query params.

---

## 2. Decisiones de diseño (sin BLOCKED)

| Decisión | Elección | Razón |
|----------|----------|-------|
| Estrategia | Cursor token opaco (page_token) + limit | Compatible con Firestore; Offset sería costoso en Firestore |
| Compatibilidad UI | Respuesta wrappea el array en `{ items, next_page_token }` con fallback transparente | Sin cambios breaking en React Query |
| Orden canónico | `created_at DESC` | Más útil para helpdesk; determinista en ambos storages |
| Límite default | 50 | Cubre todos los casos actuales de beta |
| Límite máximo | 200 | Evita abuso sin necesidad de rate limiting extra |
| Page token | Base64(JSON `{ last_id, last_created_at }`) | Opaco para el cliente; fácil de decodificar en Rust |

---

## 3. Contrato API

### 3.1 Request

```
GET /analysis-runs?limit=50&page_token=<opaque>
GET /analysis-runs/:id/threads?limit=50&page_token=<opaque>
```

Query params opcionales. Si se omiten → comportamiento actual (primeros 50).

### 3.2 Response (nuevo wrapper)

```json
{
  "items": [ /* AnalysisRun[] o EmailThread[] */ ],
  "next_page_token": "eyJsYXN0X2lkIjoiYWJjIiwi..." // null si no hay más
}
```

### 3.3 Retrocompatibilidad frontend

El cliente React Query actual recibe `AnalysisRun[]`. Migración en dos pasos:

1. **Fase A (esta tarea):** API devuelve `{ items, next_page_token }`. Frontend extrae `.items` con un selector en React Query.
2. **Fase B (próxima tarea):** UI muestra botón "Cargar más" usando `next_page_token`.

---

## 4. Cambios por capa

### 4.1 API Rust (`apps/api/`)

#### `src/http/mod.rs`
Añadir query struct compartida:

```rust
#[derive(Deserialize)]
pub struct PaginationParams {
    pub limit: Option<u32>,
    pub page_token: Option<String>,
}
```

#### `src/http/internal.rs`
- Handlers `list_analysis_runs` y `list_threads` aceptan `Query<PaginationParams>`
- Construir `PaginationOptions { limit: min(limit.unwrap_or(50), 200), cursor }` donde `cursor` = decode del `page_token`
- Devolver `PaginatedResponse<T>` en lugar de `Vec<T>`

```rust
#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
}
```

#### `src/scheduler/mod.rs` o nuevo `src/pagination.rs`
Cursor helpers:

```rust
pub fn encode_cursor(last_id: &str, last_ts: i64) -> String { /* base64 JSON */ }
pub fn decode_cursor(token: &str) -> Result<(String, i64), PaginationError>
```

### 4.2 Storage trait (`apps/api/src/storage/`)

Añadir métodos paginados al trait `Storage`:

```rust
async fn list_runs_paginated(
    &self, user_id: &str, opts: PaginationOptions
) -> Result<PaginatedPage<AnalysisRun>>;

async fn list_threads_paginated(
    &self, run_id: &str, user_id: &str, opts: PaginationOptions
) -> Result<PaginatedPage<EmailThread>>;
```

Mantener métodos no paginados existentes como wrappers que llaman al nuevo con defaults.

#### MemoryStorage
- Ordenar por `created_at DESC`
- Filtrar items donde `created_at < cursor_ts || (created_at == cursor_ts && id > cursor_id)`
- Tomar `limit + 1`, si hay +1 → generar `next_page_token`

#### FirestoreStorage
- `.order_by("created_at", Direction::Descending)`
- `.start_after([cursor_ts, cursor_id])` si hay cursor
- `.limit(limit + 1)` mismo patrón

### 4.3 Frontend React (`apps/web/`)

#### `src/api/types.ts`
```typescript
export interface PaginatedResponse<T> {
  items: T[];
  next_page_token: string | null;
}
```

#### `src/api/client.ts`
Funciones actuales devuelven `AnalysisRun[]` → cambiar a `PaginatedResponse<AnalysisRun>`:

```typescript
export async function fetchAnalysisRuns(pageToken?: string): Promise<PaginatedResponse<AnalysisRun>>
export async function fetchThreads(runId: string, pageToken?: string): Promise<PaginatedResponse<EmailThread>>
```

#### `src/App.tsx` y vistas que consuman datos
Añadir selector en `useQuery`:

```typescript
const { data } = useQuery({
  queryKey: ['analysis-runs'],
  queryFn: () => fetchAnalysisRuns(),
  select: (res) => res.items,  // ← compatibilidad: el resto del código no cambia
})
```

Esto preserva el tipo inferido como `AnalysisRun[]` en toda la UI existente.

---

## 5. Tests

### 5.1 Rust (unit + integration)

| Test | Qué verifica |
|------|-------------|
| `pagination::decode_cursor` con token válido | Decoding correcto |
| `pagination::decode_cursor` con token inválido | Error controlado, no panic |
| `MemoryStorage::list_runs_paginated` sin cursor | Primera página correcta, next_page_token presente |
| `MemoryStorage::list_runs_paginated` con cursor | Segunda página, sin duplicados, sin items previos |
| `MemoryStorage::list_runs_paginated` última página | next_page_token == null |
| Handler HTTP `GET /analysis-runs?limit=2` | Status 200, body tiene `items` y `next_page_token` |
| Handler sin params | Igual que antes (50 items max) |
| `limit > 200` | Clamped a 200 |
| `page_token` inválido | 400 Bad Request con mensaje claro |

### 5.2 TypeScript (Vitest)

| Test | Qué verifica |
|------|-------------|
| `fetchAnalysisRuns()` sin params | Llama a `/analysis-runs` sin query params |
| `fetchAnalysisRuns('tok123')` | Llama con `?page_token=tok123` |
| El tipo `PaginatedResponse<AnalysisRun>` | Compila con `items` y `next_page_token` |

---

## 6. Criterios de aceptación

- [ ] `GET /analysis-runs` sin params devuelve `{ items: [...], next_page_token: null }` cuando hay ≤50 runs
- [ ] Con `?limit=2` y 5 runs → primera respuesta tiene 2 items y `next_page_token` no nulo
- [ ] Usando ese token en segunda llamada → 2 items diferentes, sin solapamiento
- [ ] Tercera llamada con el nuevo token → 1 item y `next_page_token: null`
- [ ] `page_token` corrupto → HTTP 400, no 500
- [ ] `limit=500` → responde con máximo 200 items
- [ ] La UI actual no muestra errores ni cambios visuales (compatibilidad via `select`)
- [ ] FirestoreStorage pasa los mismos escenarios en integración
- [ ] Cobertura de tests ≥80% en el módulo de paginación

---

## 7. Orden de implementación

```
1. [Rust] Structs PaginationParams, PaginationOptions, PaginatedResponse, PaginatedPage
2. [Rust] pagination.rs: encode_cursor / decode_cursor
3. [Rust] MemoryStorage métodos paginados + tests unitarios
4. [Rust] Handlers HTTP actualizados + test de integración
5. [Rust] FirestoreStorage métodos paginados
6. [TS]  types.ts: PaginatedResponse<T>
7. [TS]  client.ts: funciones actualizadas
8. [TS]  App.tsx y vistas: añadir select: (r) => r.items
9. [TS]  Tests Vitest
10. Smoke test manual E2E con 3 páginas
```

Pasos 1-5 pueden avanzar en paralelo con pasos 6-9.

---

## 8. Riesgos y mitigaciones

| Riesgo | Impacto | Mitigación |
|--------|---------|------------|
| Cursor inválido causa 500 en producción | Alto | Validar y retornar 400 en el handler antes de llegar al storage |
| Firestore `start_after` requiere snapshot, no valores | Medio | Usar `start_after(DocumentSnapshot)` o `start_after([ts, id])` según SDK; testar explícitamente |
| MemoryStorage no está ordenado consistentemente | Medio | Sortear explícitamente antes de paginar en cada llamada |
| React Query cachea con `queryKey` fijo; páginas múltiples se sobreescriben | Bajo (beta) | En Fase B usar `useInfiniteQuery`; ahora el `select` lo aisla |
| `next_page_token: null` vs campo ausente | Bajo | Serializar siempre el campo (null explícito), nunca omitir |

---

## 9. Fuera de alcance (Fase B)

- UI "Cargar más" / scroll infinito
- `useInfiniteQuery` en React Query
- Total count en la respuesta (`total_items`)
- Paginación en otros endpoints

---

**Entregable listo para implementar.** Sin decisiones bloqueantes; todas las ambigüedades resueltas en §2.
