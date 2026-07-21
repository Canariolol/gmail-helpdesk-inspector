use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::policies::PolicySnapshot;

/// Un hilo largo pero resuelto (o respondido) dentro de esta ventana se considera
/// bien atendido y no "sospechoso": 8 horas ≈ una jornada laboral.
const SUSPICIOUS_QUICK_RESOLUTION_MINUTES: i64 = 8 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    ValidClientRequest,
    Internal,
    Automated,
    Newsletter,
    Spam,
    Misc,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationSource {
    Rules,
    Heuristics,
    Ai,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub date_from: String,
    pub date_to: String,
    #[serde(default = "default_time_from")]
    pub time_from: String,
    #[serde(default = "default_time_to")]
    pub time_to: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    pub internal_domains: Vec<String>,
    pub ignored_senders: Vec<String>,
    pub ignored_domains: Vec<String>,
    pub ignored_keywords: Vec<String>,
    /// Etiquetas/categorías de Gmail a incluir y excluir en la recuperación.
    /// Vacías = comportamiento histórico (INBOX + pestaña Principal).
    #[serde(default)]
    pub include_labels: Vec<String>,
    #[serde(default)]
    pub exclude_labels: Vec<String>,
}

fn default_time_from() -> String {
    "00:00".to_string()
}

fn default_time_to() -> String {
    "23:59".to_string()
}

fn default_timezone() -> String {
    "America/Santiago".to_string()
}

/// Count of threads per final classification, so wrongly-suppressed threads
/// (which fold into `ignored`) are visible instead of vanishing silently.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClassificationBreakdown {
    #[serde(default)]
    pub valid_client_request: u64,
    #[serde(default)]
    pub internal: u64,
    #[serde(default)]
    pub automated: u64,
    #[serde(default)]
    pub newsletter: u64,
    #[serde(default)]
    pub spam: u64,
    #[serde(default)]
    pub misc: u64,
    #[serde(default)]
    pub ambiguous: u64,
}

/// Disposición de un hilo candidato durante el procesamiento. Hace visible la
/// pérdida silenciosa: los hilos descartados antes de clasificar no se guardan
/// en ningún lado, así que sin esto no quedaba rastro de por qué "desaparecen".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadDisposition {
    #[default]
    Stored,
    DroppedNotPrimaryInbox,
    DroppedNoExternalInWindow,
    /// Hilo que SÍ era analizable (sobrevivió el embudo) pero quedó fuera porque se
    /// alcanzó el tope de hilos analizados del plan. No se guarda ni se audita.
    SkippedByPlanCap,
}

/// Metadatos mínimos de un hilo descartado antes de clasificarse, para que el
/// usuario pueda ver CUÁLES se cayeron y por qué (nunca guarda el cuerpo).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DroppedThreadInfo {
    #[serde(alias = "gmail_thread_id")]
    pub thread_id: String,
    pub subject: String,
    #[serde(default)]
    pub first_message_at: Option<DateTime<Utc>>,
    pub reason: ThreadDisposition,
}

/// Embudo de diagnóstico de un run: cuántos candidatos se descartan en cada
/// filtro previo a la clasificación, con una muestra acotada de los descartados.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AnalysisFunnel {
    #[serde(default)]
    pub dropped_not_primary_inbox: u64,
    #[serde(default)]
    pub dropped_no_external_in_window: u64,
    #[serde(default)]
    pub dropped_samples: Vec<DroppedThreadInfo>,
    /// Hilos analizables que quedaron fuera por el tope de hilos analizados del plan.
    #[serde(default)]
    pub skipped_by_plan_cap: u64,
    /// `true` cuando el tope del plan realmente recortó hilos analizables.
    #[serde(default)]
    pub truncated_by_plan: bool,
    /// Tope de hilos analizados que aplicó el plan en este run (cuando truncó).
    #[serde(default)]
    pub plan_reported_cap: Option<u32>,
    /// Total de hilos analizables observados = analizados + saltados por tope.
    #[serde(default)]
    pub would_be_reported: Option<u64>,
    /// `true` si Gmail indicó que había aún más hilos allá del lote recuperado.
    #[serde(default)]
    pub more_beyond_retrieved: bool,
    /// Hilos enviados al clasificador IA por lotes.
    #[serde(default)]
    pub ai_batch_classified: u64,
    /// Hilos que requirieron una segunda auditoría IA detallada.
    #[serde(default)]
    pub ai_detailed_audited: u64,
    /// Hilos únicos enviados a IA (una escalada no cuenta dos veces).
    #[serde(default)]
    pub ai_unique_threads: u64,
    /// Llamadas totales al proveedor IA, incluidos reintentos y escaladas.
    #[serde(default)]
    pub ai_calls: u64,
    /// Hilos que iban a auditarse pero quedaron fuera al agotarse el cupo mensual
    /// de IA del plan. Conservan su clasificación heurística.
    #[serde(default)]
    pub ai_skipped_by_budget: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisMetrics {
    pub total_threads: u64,
    pub valid_requests: u64,
    pub answered: u64,
    pub unanswered: u64,
    pub ignored: u64,
    pub ambiguous: u64,
    #[serde(default)]
    pub pending_review: u64,
    pub manual_overrides: u64,
    pub avg_first_response_minutes: Option<f64>,
    pub median_first_response_minutes: Option<f64>,
    pub p90_first_response_minutes: Option<f64>,
    #[serde(default)]
    pub avg_resolution_minutes: Option<f64>,
    #[serde(default)]
    pub median_resolution_minutes: Option<f64>,
    pub report_confidence: f64,
    pub ai_input_tokens: u64,
    pub ai_output_tokens: u64,
    #[serde(default)]
    pub classification_breakdown: ClassificationBreakdown,
    /// Embudo de diagnóstico del run (descartes previos a la clasificación).
    #[serde(default)]
    pub funnel: AnalysisFunnel,
}

impl Default for AnalysisMetrics {
    fn default() -> Self {
        Self {
            total_threads: 0,
            valid_requests: 0,
            answered: 0,
            unanswered: 0,
            ignored: 0,
            ambiguous: 0,
            pending_review: 0,
            manual_overrides: 0,
            avg_first_response_minutes: None,
            median_first_response_minutes: None,
            p90_first_response_minutes: None,
            avg_resolution_minutes: None,
            median_resolution_minutes: None,
            report_confidence: 1.0,
            ai_input_tokens: 0,
            ai_output_tokens: 0,
            classification_breakdown: ClassificationBreakdown::default(),
            funnel: AnalysisFunnel::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    Manual,
    Scheduled,
    Backfill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRun {
    pub id: String,
    pub user_email: String,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub mailbox_id: Option<String>,
    #[serde(default)]
    pub trigger_type: Option<TriggerType>,
    #[serde(default)]
    pub policy_version_id: Option<String>,
    #[serde(default)]
    pub policy_hash: Option<String>,
    #[serde(default)]
    pub policy_snapshot: Option<PolicySnapshot>,
    #[serde(default)]
    pub gmail_scope_snapshot: Vec<String>,
    #[serde(default)]
    pub retention_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub data_minimization_mode: Option<String>,
    pub config: AnalysisConfig,
    pub status: AnalysisStatus,
    pub progress_message: String,
    pub processed_threads: u64,
    pub total_candidate_threads: u64,
    pub metrics: AnalysisMetrics,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub id: String,
    #[serde(alias = "gmail_message_id")]
    pub message_id: String,
    pub from_email: String,
    pub from_name: Option<String>,
    pub to_emails: Vec<String>,
    pub cc_emails: Vec<String>,
    pub date: DateTime<Utc>,
    pub subject: String,
    pub snippet: String,
    pub headers: serde_json::Value,
    pub is_internal: bool,
    pub is_external: bool,
    pub is_automated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailThread {
    pub id: String,
    pub analysis_run_id: String,
    #[serde(alias = "gmail_thread_id")]
    pub thread_id: String,
    pub subject: String,
    pub normalized_subject: String,
    pub classification: Classification,
    pub classification_source: ClassificationSource,
    pub classification_confidence: f64,
    pub is_valid_client_request: bool,
    pub is_answered: bool,
    #[serde(default)]
    pub first_message_at: Option<DateTime<Utc>>,
    pub first_client_message_id: Option<String>,
    pub first_internal_reply_message_id: Option<String>,
    #[serde(default)]
    pub last_internal_message_id: Option<String>,
    pub first_client_message_at: Option<DateTime<Utc>>,
    pub first_internal_reply_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_internal_message_at: Option<DateTime<Utc>>,
    pub response_time_minutes: Option<i64>,
    #[serde(default)]
    pub resolution_time_minutes: Option<i64>,
    pub manual_review_required: bool,
    pub manual_override_applied: bool,
    pub reasons: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAuditResult {
    #[serde(default)]
    pub policy_version_id: Option<String>,
    #[serde(default)]
    pub prompt_version: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub auto_apply_threshold: Option<f64>,
    #[serde(default)]
    pub max_audit_messages: Option<u32>,
    #[serde(default)]
    pub max_body_chars_per_message: Option<u32>,
    pub classification: Classification,
    pub is_valid_client_request: bool,
    pub is_answered: bool,
    pub first_client_message_id: Option<String>,
    pub first_internal_reply_message_id: Option<String>,
    #[serde(default)]
    pub last_internal_message_id: Option<String>,
    pub confidence: f64,
    pub manual_review_required: bool,
    pub issues: Vec<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualReview {
    pub id: String,
    pub email_thread_id: String,
    pub reviewer_label: String,
    pub new_classification: Classification,
    pub is_valid_client_request: bool,
    pub is_answered: bool,
    pub first_client_message_id: Option<String>,
    pub first_internal_reply_message_id: Option<String>,
    pub last_internal_message_id: Option<String>,
    pub first_client_message_at: Option<DateTime<Utc>>,
    pub first_internal_reply_at: Option<DateTime<Utc>>,
    pub last_internal_message_at: Option<DateTime<Utc>>,
    pub response_time_minutes: Option<i64>,
    pub resolution_time_minutes: Option<i64>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualReviewOverride {
    pub owner_email: String,
    #[serde(alias = "gmail_thread_id")]
    pub thread_id: String,
    pub source_run_id: String,
    pub message_fingerprint: String,
    pub reviewer_label: String,
    pub classification: Classification,
    pub is_answered: bool,
    pub first_client_message_id: Option<String>,
    pub first_internal_reply_message_id: Option<String>,
    pub last_internal_message_id: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub fn message_fingerprint(messages: &[EmailMessage]) -> String {
    let mut ids = messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    let mut hasher = Sha256::new();
    for id in ids {
        hasher.update(id.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn apply_manual_review_override(
    thread: &mut EmailThread,
    messages: &[EmailMessage],
    review: &ManualReviewOverride,
) {
    thread.classification = review.classification.clone();
    thread.classification_source = ClassificationSource::Manual;
    thread.classification_confidence = 1.0;
    thread.is_valid_client_request = thread.classification == Classification::ValidClientRequest;
    thread.is_answered = review.is_answered;
    thread.first_client_message_id = review.first_client_message_id.clone();
    thread.first_internal_reply_message_id = review.first_internal_reply_message_id.clone();
    thread.last_internal_message_id = review.last_internal_message_id.clone();
    thread.first_client_message_at = thread
        .first_client_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    thread.first_internal_reply_at = thread
        .first_internal_reply_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    thread.last_internal_message_at = thread
        .last_internal_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    thread.response_time_minutes = thread
        .first_client_message_at
        .zip(thread.first_internal_reply_at)
        .map(|(client, reply)| (reply - client).num_minutes());
    thread.resolution_time_minutes = thread
        .first_client_message_at
        .zip(thread.last_internal_message_at)
        .map(|(client, last)| (last - client).num_minutes());
    thread.notes = review.notes.clone();
    thread.manual_review_required = false;
    thread.manual_override_applied = true;
    thread.reasons.push(
        "Se heredó una revisión manual previa porque el hilo no tuvo mensajes nuevos.".to_string(),
    );
    thread.updated_at = Utc::now();
}

/// Normaliza documentos creados por la versión antigua del formulario, donde
/// clasificación y validez podían guardarse de forma independiente.
///
/// La única excepción de migración es `Ambiguous + is_valid=true`: esa
/// combinación solo podía expresar que el usuario marcó la antigua casilla
/// "Solicitud válida" sin cambiar el desplegable, por lo que se promueve a
/// `ValidClientRequest`. Las notas nunca se interpretan.
pub fn reconcile_legacy_thread_classification(thread: &mut EmailThread) -> bool {
    let original_classification = thread.classification.clone();
    let original_is_valid = thread.is_valid_client_request;
    let original_manual_review_required = thread.manual_review_required;

    if thread.classification == Classification::Ambiguous && thread.is_valid_client_request {
        thread.classification = Classification::ValidClientRequest;
        thread.manual_review_required = false;
    }
    thread.is_valid_client_request = thread.classification == Classification::ValidClientRequest;

    let changed = thread.classification != original_classification
        || thread.is_valid_client_request != original_is_valid
        || thread.manual_review_required != original_manual_review_required;
    if changed {
        thread.updated_at = Utc::now();
    }
    changed
}

pub fn normalize_subject(subject: &str) -> String {
    let mut value = subject.trim().to_lowercase();
    loop {
        let next = value
            .strip_prefix("re:")
            .or_else(|| value.strip_prefix("fw:"))
            .or_else(|| value.strip_prefix("fwd:"))
            .map(|s| s.trim().to_string());
        match next {
            Some(next) if next != value => value = next,
            _ => break,
        }
    }
    value
}

pub fn classify_thread(
    analysis_run_id: &str,
    thread_id: &str,
    messages: &[EmailMessage],
    config: &AnalysisConfig,
) -> EmailThread {
    let now = Utc::now();
    let subject = messages
        .first()
        .map(|m| m.subject.clone())
        .unwrap_or_else(|| "(sin asunto)".to_string());
    let normalized_subject = normalize_subject(&subject);
    let mut reasons = Vec::new();
    let lower_subject = subject.to_lowercase();

    if messages.is_empty() {
        reasons.push("El hilo no tiene mensajes legibles.".to_string());
        return build_thread(
            analysis_run_id,
            thread_id,
            subject,
            normalized_subject,
            Classification::Ambiguous,
            ClassificationSource::Rules,
            0.2,
            false,
            false,
            None,
            None,
            None,
            None,
            true,
            reasons,
            now,
        );
    }

    let first_message = messages
        .iter()
        .min_by_key(|message| message.date)
        .expect("messages.is_empty was checked above");

    // "Request received in the window": the earliest human client (external,
    // non-automated) message dated INSIDE the analysis window. The report counts these.
    // process_one_thread already skips threads with no external message in the window, so
    // reaching this with `request == None` means the in-window external activity is
    // automated/system mail (or only an internal reply to an older request): not a
    // received request, so it is classified for visibility but neither counted nor audited.
    let request = messages
        .iter()
        .filter(|m| {
            m.is_external && !m.is_automated && message_is_inside_analysis_window(m, config)
        })
        .min_by_key(|m| m.date);

    let Some(request) = request else {
        let (classification, reason) = if messages.iter().all(|m| m.is_internal) {
            (
                Classification::Internal,
                "Solo participantes internos dentro de la ventana; no hay solicitud de cliente.",
            )
        } else {
            (
                Classification::Automated,
                "Sin mensaje de cliente humano dentro de la ventana (automático/sistema o actividad de otra ventana).",
            )
        };
        reasons.push(reason.to_string());
        return build_thread(
            analysis_run_id,
            thread_id,
            subject,
            normalized_subject,
            classification,
            ClassificationSource::Rules,
            0.9,
            false,
            false,
            None,
            None,
            None,
            Some(first_message.date),
            false,
            reasons,
            now,
        );
    };

    // The in-window client is explicitly configured as ignored → respect it.
    if sender_is_ignored(
        &request.from_email,
        &config.ignored_senders,
        &config.ignored_domains,
    ) {
        reasons.push(
            "El cliente de la solicitud está en la lista de remitentes/dominios ignorados."
                .to_string(),
        );
        return ignored_thread(
            analysis_run_id,
            thread_id,
            subject,
            normalized_subject,
            Some(first_message.date),
            reasons,
            now,
        );
    }

    // Boundary detection: the earliest client message of the whole thread may predate the
    // window. If an earlier cycle existed and was answered before this in-window request →
    // the client is re-engaging → send to review. If it was never answered → still a valid
    // pending request the client is chasing.
    let first_client_overall = messages
        .iter()
        .filter(|m| m.is_external && !m.is_automated)
        .min_by_key(|m| m.date)
        .unwrap_or(request);
    let reopened = request.date > first_client_overall.date;
    let prior_answered = reopened
        && messages.iter().any(|m| {
            m.is_internal
                && !m.is_automated
                && m.date > first_client_overall.date
                && m.date < request.date
        });

    // Response milestones are measured against the in-window request, not the whole-thread
    // first message, so a boundary-crossing thread does not pollute the window's metrics.
    let first_reply = messages
        .iter()
        .filter(|m| m.is_internal && !m.is_automated && m.date > request.date)
        .min_by_key(|m| m.date);
    let last_internal = messages
        .iter()
        .filter(|m| m.is_internal && !m.is_automated && m.date > request.date)
        .max_by_key(|m| m.date);
    let response_minutes = first_reply.map(|reply| (reply.date - request.date).num_minutes());
    let resolution_minutes =
        last_internal.map(|message| (message.date - request.date).num_minutes());

    let mut noise = false;
    if first_message.is_internal {
        noise = true;
        reasons.push(
            "El hilo abre con un mensaje interno; el cliente humano aparece después.".to_string(),
        );
    }
    if first_message.is_automated {
        noise = true;
        reasons.push(
            "El primer mensaje parece automático, pero el hilo incluye un cliente humano."
                .to_string(),
        );
    }
    if subject_has_ignored_keyword(&lower_subject, &config.ignored_keywords) {
        noise = true;
        reasons.push("El asunto coincide con una palabra ignorada.".to_string());
    }
    if subject_looks_like_newsletter(&lower_subject) {
        noise = true;
        reasons.push("El asunto parece un boletín o promoción.".to_string());
    }
    if reopened && prior_answered {
        noise = true;
        reasons.push(
            "El hilo se reabrió: ya tenía respuesta previa y el cliente volvió a escribir dentro de la ventana; requiere revisión."
                .to_string(),
        );
    } else if reopened {
        reasons.push(
            "El cliente retomó dentro de la ventana una solicitud previa que no había sido respondida."
                .to_string(),
        );
    }

    // "Forma atípica": muchos mensajes o muchas idas y vueltas externas. Pero un hilo
    // largo que igualmente se resolvió con prontitud (respuesta o cierre interno dentro de
    // SUSPICIOUS_QUICK_RESOLUTION_MINUTES) refleja una conversación sana, no un caso confuso:
    // no lo marcamos sospechoso para no enviar a revisión hilos ya atendidos bien.
    let long_shape = messages.len() >= 8 || messages.iter().filter(|m| m.is_external).count() >= 4;
    let handled_promptly = resolution_minutes
        .or(response_minutes)
        .map(|minutes| (0..=SUSPICIOUS_QUICK_RESOLUTION_MINUTES).contains(&minutes))
        .unwrap_or(false);
    let suspicious = long_shape && !handled_promptly;

    reasons.push("El primer mensaje relevante viene de un remitente externo humano.".to_string());
    if first_reply.is_some() {
        reasons.push("Se encontró una respuesta interna posterior no automática.".to_string());
    } else {
        reasons.push("No se encontró una respuesta interna posterior.".to_string());
    }
    if suspicious {
        reasons.push("La forma del hilo es atípica y conviene auditarla.".to_string());
    }

    let (classification, confidence, is_valid) = if noise {
        (Classification::Ambiguous, 0.5, false)
    } else if suspicious {
        (Classification::ValidClientRequest, 0.72, true)
    } else {
        (Classification::ValidClientRequest, 0.86, true)
    };
    let manual_review_required = noise || suspicious;

    build_thread(
        analysis_run_id,
        thread_id,
        subject,
        normalized_subject,
        classification,
        ClassificationSource::Heuristics,
        confidence,
        is_valid,
        first_reply.is_some(),
        Some(request.id.clone()),
        first_reply.map(|m| m.id.clone()),
        last_internal.map(|m| m.id.clone()),
        Some(first_message.date),
        manual_review_required,
        reasons,
        now,
    )
    .with_dates(
        Some(request.date),
        first_reply.map(|m| m.date),
        last_internal.map(|m| m.date),
        response_minutes,
        resolution_minutes,
    )
}

/// Coincidencia de palabra clave sobre un texto ya en minúsculas. Las keywords
/// multi-palabra hacen match por substring; las de una sola palabra, por límite
/// de palabra (para que "ticket" no marque "ticketera" ni "boletines").
fn keyword_matches(lower_haystack: &str, keywords: &[String]) -> bool {
    keywords.iter().any(|kw| {
        let kw = kw.trim().to_lowercase();
        if kw.is_empty() {
            false
        } else if kw.contains(' ') {
            lower_haystack.contains(kw.as_str())
        } else {
            lower_haystack
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word == kw.as_str())
        }
    })
}

fn subject_has_ignored_keyword(lower_subject: &str, keywords: &[String]) -> bool {
    keyword_matches(lower_subject, keywords)
}

fn subject_looks_like_newsletter(lower_subject: &str) -> bool {
    ["newsletter", "boletin", "boletín"].iter().any(|kw| {
        lower_subject
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word == *kw)
    })
}

fn ignored_thread(
    analysis_run_id: &str,
    thread_id: &str,
    subject: String,
    normalized_subject: String,
    first_message_at: Option<DateTime<Utc>>,
    reasons: Vec<String>,
    now: DateTime<Utc>,
) -> EmailThread {
    build_thread(
        analysis_run_id,
        thread_id,
        subject,
        normalized_subject,
        Classification::Misc,
        ClassificationSource::Rules,
        0.9,
        false,
        false,
        None,
        None,
        None,
        first_message_at,
        false,
        reasons,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_thread(
    analysis_run_id: &str,
    thread_id: &str,
    subject: String,
    normalized_subject: String,
    classification: Classification,
    classification_source: ClassificationSource,
    classification_confidence: f64,
    is_valid_client_request: bool,
    is_answered: bool,
    first_client_message_id: Option<String>,
    first_internal_reply_message_id: Option<String>,
    last_internal_message_id: Option<String>,
    first_message_at: Option<DateTime<Utc>>,
    manual_review_required: bool,
    reasons: Vec<String>,
    now: DateTime<Utc>,
) -> EmailThread {
    EmailThread {
        id: thread_id.to_string(),
        analysis_run_id: analysis_run_id.to_string(),
        thread_id: thread_id.to_string(),
        subject,
        normalized_subject,
        classification,
        classification_source,
        classification_confidence,
        is_valid_client_request,
        is_answered,
        first_message_at,
        first_client_message_id,
        first_internal_reply_message_id,
        last_internal_message_id,
        first_client_message_at: None,
        first_internal_reply_at: None,
        last_internal_message_at: None,
        response_time_minutes: None,
        resolution_time_minutes: None,
        manual_review_required,
        manual_override_applied: false,
        reasons,
        notes: None,
        created_at: now,
        updated_at: now,
    }
}

trait WithDates {
    fn with_dates(
        self,
        first_client_message_at: Option<DateTime<Utc>>,
        first_internal_reply_at: Option<DateTime<Utc>>,
        last_internal_message_at: Option<DateTime<Utc>>,
        response_time_minutes: Option<i64>,
        resolution_time_minutes: Option<i64>,
    ) -> Self;
}

impl WithDates for EmailThread {
    fn with_dates(
        mut self,
        first_client_message_at: Option<DateTime<Utc>>,
        first_internal_reply_at: Option<DateTime<Utc>>,
        last_internal_message_at: Option<DateTime<Utc>>,
        response_time_minutes: Option<i64>,
        resolution_time_minutes: Option<i64>,
    ) -> Self {
        self.first_client_message_at = first_client_message_at;
        self.first_internal_reply_at = first_internal_reply_at;
        self.last_internal_message_at = last_internal_message_at;
        self.response_time_minutes = response_time_minutes;
        self.resolution_time_minutes = resolution_time_minutes;
        self
    }
}

pub fn sender_is_ignored(
    email: &str,
    ignored_senders: &[String],
    ignored_domains: &[String],
) -> bool {
    let lower = email.to_lowercase();
    ignored_senders.iter().any(|s| s.to_lowercase() == lower)
        || ignored_domains
            .iter()
            .any(|domain| email_matches_domain(&lower, domain))
}

pub fn is_automated_sender(email: &str, headers: &serde_json::Value) -> bool {
    // Match on the local-part with anchored patterns instead of an unanchored
    // `contains`, so real client addresses like `notifications@cliente.cl` are NOT
    // wrongly flagged. `List-Unsubscribe` presence is intentionally NOT used: legitimate
    // corporate/ticketing platforms (Workspace, HubSpot, Zendesk) add it to human mail.
    let lower = email.to_lowercase();
    let local_part = lower.split('@').next().unwrap_or(lower.as_str());
    let automated_local = matches!(
        local_part,
        "no-reply" | "noreply" | "no_reply" | "donotreply" | "do-not-reply" | "mailer-daemon"
    ) || local_part.starts_with("no-reply")
        || local_part.starts_with("noreply")
        || local_part.starts_with("donotreply");
    automated_local
        || headers
            .get("auto-submitted")
            .and_then(|v| v.as_str())
            .map(|value| {
                // RFC 3834: solo los tokens "auto-*" (auto-generated, auto-replied,
                // auto-notified) indican correo automático. El valor "no" y otros valores
                // no canónicos (p. ej. "yes", "true", "1") que algunas plataformas setean por
                // error ya NO marcan el mensaje como automático: reduce falsos `Automated`.
                value.trim().to_lowercase().starts_with("auto-")
            })
            .unwrap_or(false)
}

pub fn is_internal_email(email: &str, domains: &[String]) -> bool {
    let lower = email.to_lowercase();
    domains
        .iter()
        .any(|domain| email_matches_domain(&lower, domain))
}

fn email_matches_domain(lower_email: &str, domain: &str) -> bool {
    let normalized = domain.trim().trim_start_matches('@').to_lowercase();
    !normalized.is_empty() && lower_email.ends_with(&format!("@{normalized}"))
}

pub fn message_is_inside_analysis_window(message: &EmailMessage, config: &AnalysisConfig) -> bool {
    let timezone = config
        .timezone
        .parse::<Tz>()
        .unwrap_or(chrono_tz::America::Santiago);
    let local = message.date.with_timezone(&timezone);
    let Ok(date_from) = chrono::NaiveDate::parse_from_str(&config.date_from, "%Y-%m-%d") else {
        return true;
    };
    let Ok(date_to) = chrono::NaiveDate::parse_from_str(&config.date_to, "%Y-%m-%d") else {
        return true;
    };
    let time_from = NaiveTime::parse_from_str(&config.time_from, "%H:%M").unwrap_or(NaiveTime::MIN);
    let time_to = NaiveTime::parse_from_str(&config.time_to, "%H:%M")
        .unwrap_or(NaiveTime::from_hms_opt(23, 59, 59).expect("valid default time"));

    local.date_naive() >= date_from
        && local.date_naive() <= date_to
        && local.time() >= time_from
        && local.time() <= time_to
}

pub fn calculate_metrics(
    threads: &[EmailThread],
    ai_input_tokens: u64,
    ai_output_tokens: u64,
) -> AnalysisMetrics {
    let total_threads = threads.len() as u64;
    let valid: Vec<_> = threads
        .iter()
        .filter(|t| t.classification == Classification::ValidClientRequest)
        .collect();
    let valid_requests = valid.len() as u64;
    let answered = valid.iter().filter(|t| t.is_answered).count() as u64;
    let ambiguous = threads
        .iter()
        .filter(|t| t.classification == Classification::Ambiguous)
        .count() as u64;
    let pending_review = threads.iter().filter(|t| t.manual_review_required).count() as u64;
    let ignored = threads
        .iter()
        .filter(|t| {
            t.classification != Classification::ValidClientRequest
                && t.classification != Classification::Ambiguous
        })
        .count() as u64;
    let manual_overrides = threads.iter().filter(|t| t.manual_override_applied).count() as u64;
    let mut response_times: Vec<f64> = valid
        .iter()
        .filter_map(|t| t.response_time_minutes.map(|v| v as f64))
        .collect();
    response_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut resolution_times: Vec<f64> = valid
        .iter()
        .filter_map(|t| t.resolution_time_minutes.map(|v| v as f64))
        .collect();
    resolution_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let confidence = if total_threads == 0 {
        1.0
    } else {
        (1.0 - (pending_review as f64 / total_threads as f64)).clamp(0.0, 1.0)
    };

    let mut classification_breakdown = ClassificationBreakdown::default();
    for thread in threads {
        match thread.classification {
            Classification::ValidClientRequest => {
                classification_breakdown.valid_client_request += 1
            }
            Classification::Internal => classification_breakdown.internal += 1,
            Classification::Automated => classification_breakdown.automated += 1,
            Classification::Newsletter => classification_breakdown.newsletter += 1,
            Classification::Spam => classification_breakdown.spam += 1,
            Classification::Misc => classification_breakdown.misc += 1,
            Classification::Ambiguous => classification_breakdown.ambiguous += 1,
        }
    }

    AnalysisMetrics {
        total_threads,
        valid_requests,
        answered,
        unanswered: valid_requests.saturating_sub(answered),
        ignored,
        ambiguous,
        pending_review,
        manual_overrides,
        avg_first_response_minutes: average(&response_times),
        median_first_response_minutes: percentile(&response_times, 0.5),
        p90_first_response_minutes: percentile(&response_times, 0.9),
        avg_resolution_minutes: average(&resolution_times),
        median_resolution_minutes: percentile(&resolution_times, 0.5),
        report_confidence: confidence,
        ai_input_tokens,
        ai_output_tokens,
        classification_breakdown,
        // El embudo se rellena en el loop del run (los descartados no están en
        // `threads`, así que aquí queda por defecto y se setea después).
        funnel: AnalysisFunnel::default(),
    }
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(idx).copied()
}

/// Categorías/pestañas de Gmail que rara vez corresponden a una solicitud de
/// soporte válida; se usan como señal para refinar la clasificación heurística.
const GMAIL_PROMOTIONS_LABEL: &str = "CATEGORY_PROMOTIONS";
const GMAIL_SOCIAL_LABEL: &str = "CATEGORY_SOCIAL";
const GMAIL_FORUMS_LABEL: &str = "CATEGORY_FORUMS";

/// Refina la clasificación usando las etiquetas propias de Gmail. Es
/// conservador: ante un conflicto con un veredicto "válido" de reglas/heurística
/// no invierte a ciegas, sino que enruta a revisión manual (y, para Promociones,
/// marca Newsletter). No toca resultados ya aplicados por IA o revisión manual.
pub fn refine_classification_with_folders(thread: &mut EmailThread, gmail_labels: &[String]) {
    if matches!(
        thread.classification_source,
        ClassificationSource::Ai | ClassificationSource::Manual
    ) {
        return;
    }
    let has_label = |needle: &str| {
        gmail_labels
            .iter()
            .any(|label| label.eq_ignore_ascii_case(needle))
    };

    if thread.is_valid_client_request && has_label(GMAIL_PROMOTIONS_LABEL) {
        thread.classification = Classification::Newsletter;
        thread.is_valid_client_request = false;
        thread.manual_review_required = true;
        thread.reasons.push(
            "Gmail ubicó el hilo en la pestaña Promociones; se marca para revisión por posible boletín/promoción."
                .to_string(),
        );
    } else if thread.is_valid_client_request
        && (has_label(GMAIL_SOCIAL_LABEL) || has_label(GMAIL_FORUMS_LABEL))
    {
        thread.manual_review_required = true;
        thread.reasons.push(
            "Gmail ubicó el hilo en la pestaña Social/Foros; requiere revisión para confirmar que es una solicitud válida."
                .to_string(),
        );
    }
}

/// Palabras demasiado genéricas para distinguir una regla de no-responsabilidad;
/// se descartan al extraer los términos significativos de cada regla.
const RULE_STOPWORDS: &[&str] = &[
    "para",
    "con",
    "los",
    "las",
    "una",
    "unos",
    "unas",
    "del",
    "que",
    "son",
    "este",
    "esta",
    "esto",
    "estos",
    "estas",
    "como",
    "por",
    "sobre",
    "pero",
    "nuestra",
    "nuestro",
    "nuestros",
    "nuestras",
    "responsabilidad",
    "responsabilidades",
    "tema",
    "temas",
    "asunto",
    "asuntos",
    "correo",
    "correos",
    "mensaje",
    "mensajes",
    "cliente",
    "clientes",
    "solicitud",
    "solicitudes",
    "atendemos",
    "atender",
    "corresponde",
    "nosotros",
    "ellos",
    "todo",
    "toda",
    "todos",
    "todas",
];

/// Extrae los términos distintivos de una regla de texto libre: palabras de 4+
/// caracteres que no sean genéricas, en minúsculas y sin duplicados.
fn significant_terms(text: &str) -> std::collections::BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4)
        .map(|word| word.to_lowercase())
        .filter(|word| !RULE_STOPWORDS.contains(&word.as_str()))
        .collect()
}

/// Una regla "coincide claramente" con el texto del hilo cuando: si tiene un solo
/// término distintivo, ese término aparece como palabra; si tiene varios, al menos
/// dos términos distintos coinciden (exige dos señales para reducir falsos positivos).
fn rule_matches_haystack(rule: &str, haystack_words: &std::collections::HashSet<&str>) -> bool {
    let terms = significant_terms(rule);
    if terms.is_empty() {
        return false;
    }
    let hits = terms
        .iter()
        .filter(|term| haystack_words.contains(term.as_str()))
        .count();
    let required = if terms.len() == 1 { 1 } else { 2 };
    hits >= required
}

/// Refina la clasificación heurística usando las reglas de no-responsabilidad de la
/// política (que hoy solo alimentaban a la IA). Conservadora: solo enruta a revisión
/// manual los hilos hoy considerados válidos y aún sin marcar cuando su asunto o
/// remitente coincide claramente con una regla. Nunca invierte la clasificación ni la
/// validez, y no toca resultados ya aplicados por IA o revisión manual.
pub fn refine_classification_with_policy_hints(
    thread: &mut EmailThread,
    request_sender_email: &str,
    non_responsibility_rules: &[String],
) {
    if matches!(
        thread.classification_source,
        ClassificationSource::Ai | ClassificationSource::Manual
    ) {
        return;
    }
    if !thread.is_valid_client_request || thread.manual_review_required {
        return;
    }
    let haystack_owned = format!(
        "{} {}",
        thread.subject.to_lowercase(),
        request_sender_email.to_lowercase()
    );
    let haystack_words: std::collections::HashSet<&str> = haystack_owned
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let matched = non_responsibility_rules
        .iter()
        .any(|rule| rule_matches_haystack(rule, &haystack_words));
    if matched {
        thread.manual_review_required = true;
        thread.reasons.push(
            "El asunto o remitente coincide con una regla de no-responsabilidad configurada; se marca para revisión."
                .to_string(),
        );
    }
}

/// Rescata a Solicitud Válida los hilos cuyo asunto o cuerpo menciona una "señal
/// de ticket" configurada por el tenant (p. ej. "ticket", "incidencia"), y los deja
/// marcados para que la auditoría IA dé el veredicto final. Pensada para hilos que
/// la heurística dejó como `Automated`/`Internal`/`Misc`/`Ambiguous` pero que el
/// equipo sí tramitó. No toca hilos ya válidos ni resueltos por IA/revisión manual.
pub fn rescue_classification_with_valid_signals(
    thread: &mut EmailThread,
    messages: &[EmailMessage],
    keywords: &[String],
) {
    if keywords.is_empty() || thread.is_valid_client_request {
        return;
    }
    if matches!(
        thread.classification_source,
        ClassificationSource::Ai | ClassificationSource::Manual
    ) {
        return;
    }
    let subject_hit = keyword_matches(&thread.subject.to_lowercase(), keywords);
    let body_hit = || {
        messages.iter().any(|message| {
            message
                .body_text
                .as_deref()
                .map(|body| keyword_matches(&body.to_lowercase(), keywords))
                .unwrap_or(false)
        })
    };
    if !subject_hit && !body_hit() {
        return;
    }
    // Promueve a válido y deja la decisión final a la IA (manual_review_required
    // habilita la auditoría aunque el hilo no tuviera un cliente humano detectado).
    thread.classification = Classification::ValidClientRequest;
    thread.classification_source = ClassificationSource::Heuristics;
    thread.is_valid_client_request = true;
    thread.manual_review_required = true;
    // Ancla al primer mensaje externo del hilo para que `received_human_thread`
    // sea verdadero y la auditoría IA evalúe el rescate.
    if thread.first_client_message_id.is_none()
        && let Some(anchor) = messages
            .iter()
            .filter(|message| message.is_external)
            .min_by_key(|message| message.date)
    {
        thread.first_client_message_id = Some(anchor.id.clone());
        thread.first_client_message_at = Some(anchor.date);
    }
    thread.reasons.push(
        "Rescatado como solicitud válida por una palabra de señal de ticket; pendiente de confirmación de la IA."
            .to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Los registros ya persistidos en `mira.records` usan los nombres antiguos
    /// `gmail_thread_id`/`gmail_message_id`. El renombre a nombres neutrales solo
    /// es seguro mientras los alias sigan leyéndolos: sin esto, los runs
    /// históricos dejarían de deserializar.
    #[test]
    fn reads_legacy_gmail_prefixed_ids_from_stored_json() {
        let dropped: DroppedThreadInfo = serde_json::from_value(serde_json::json!({
            "gmail_thread_id": "t1",
            "subject": "asunto",
            "reason": "dropped_not_primary_inbox",
        }))
        .expect("DroppedThreadInfo legacy");
        assert_eq!(dropped.thread_id, "t1");

        let message: EmailMessage = serde_json::from_value(serde_json::json!({
            "id": "m1",
            "gmail_message_id": "gm1",
            "from_email": "a@x.cl",
            "from_name": null,
            "to_emails": [],
            "cc_emails": [],
            "date": "2026-01-01T00:00:00Z",
            "subject": "s",
            "snippet": "",
            "headers": {},
            "is_internal": false,
            "is_external": true,
            "is_automated": false,
        }))
        .expect("EmailMessage legacy");
        assert_eq!(message.message_id, "gm1");

        let override_json = serde_json::json!({
            "owner_email": "o@x.cl",
            "gmail_thread_id": "t2",
            "source_run_id": "r1",
            "message_fingerprint": "f",
            "reviewer_label": "l",
            "classification": "valid_client_request",
            "is_answered": true,
            "first_client_message_id": null,
            "first_internal_reply_message_id": null,
            "last_internal_message_id": null,
            "notes": null,
            "response_time_minutes": null,
            "resolution_time_minutes": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
        });
        let review: ManualReviewOverride =
            serde_json::from_value(override_json).expect("ManualReviewOverride legacy");
        assert_eq!(review.thread_id, "t2");
    }

    fn msg(id: &str, from: &str, internal: bool, at_minute: i64) -> EmailMessage {
        EmailMessage {
            id: id.to_string(),
            message_id: id.to_string(),
            from_email: from.to_string(),
            from_name: None,
            to_emails: vec![],
            cc_emails: vec![],
            // Anchored inside the 2026-06 test windows so window-aware classification
            // (classify_thread requires an in-window client message) sees these messages.
            date: DateTime::from_timestamp(1_780_660_800 + at_minute * 60, 0).unwrap(),
            subject: "Ayuda con pedido".to_string(),
            snippet: "hola".to_string(),
            headers: serde_json::json!({}),
            is_internal: internal,
            is_external: !internal,
            is_automated: false,
            body_text: None,
        }
    }

    #[test]
    fn classifies_answered_external_request() {
        let config = AnalysisConfig {
            date_from: "2026-06-01".to_string(),
            date_to: "2026-06-12".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["company.test".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        };
        let thread = classify_thread(
            "run",
            "thread",
            &[
                msg("m1", "client@example.com", false, 0),
                msg("m2", "agent@company.test", true, 25),
            ],
            &config,
        );
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_answered);
        assert_eq!(thread.response_time_minutes, Some(25));
        assert_eq!(thread.resolution_time_minutes, Some(25));
    }

    #[test]
    fn metrics_use_valid_requests_only_for_unanswered() {
        let mut answered = classify_thread(
            "run",
            "a",
            &[
                msg("m1", "client@example.com", false, 0),
                msg("m2", "agent@company.test", true, 20),
            ],
            &AnalysisConfig {
                date_from: "2026-06-01".to_string(),
                date_to: "2026-06-12".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec![],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
        );
        answered.response_time_minutes = Some(20);
        let ignored = EmailThread {
            classification: Classification::Misc,
            is_valid_client_request: false,
            ..answered.clone()
        };
        let metrics = calculate_metrics(&[answered, ignored], 12, 4);
        assert_eq!(metrics.valid_requests, 1);
        assert_eq!(metrics.answered, 1);
        assert_eq!(metrics.unanswered, 0);
        assert_eq!(metrics.ai_input_tokens, 12);
    }

    #[test]
    fn pending_review_is_separate_from_exclusive_classification_totals() {
        let base = valid_thread_with_subject("Solicitud");
        let valid_pending = EmailThread {
            manual_review_required: true,
            ..base.clone()
        };
        let ignored_pending = EmailThread {
            classification: Classification::Misc,
            is_valid_client_request: false,
            manual_review_required: true,
            ..base.clone()
        };
        let ambiguous = EmailThread {
            classification: Classification::Ambiguous,
            is_valid_client_request: false,
            manual_review_required: true,
            ..base
        };
        let metrics = calculate_metrics(&[valid_pending, ignored_pending, ambiguous], 0, 0);

        assert_eq!(metrics.total_threads, 3);
        assert_eq!(metrics.valid_requests, 1);
        assert_eq!(metrics.ignored, 1);
        assert_eq!(metrics.ambiguous, 1);
        assert_eq!(metrics.pending_review, 3);
        assert_eq!(
            metrics.valid_requests + metrics.ignored + metrics.ambiguous,
            metrics.total_threads
        );
        assert_eq!(metrics.report_confidence, 0.0);
    }

    #[test]
    fn stores_first_message_timestamp_for_non_valid_threads() {
        let message = msg("m1", "agent@company.test", true, 0);
        let expected_at = message.date;
        let thread = classify_thread(
            "run",
            "outbound",
            &[message],
            &AnalysisConfig {
                date_from: "2026-06-01".to_string(),
                date_to: "2026-06-12".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec!["company.test".to_string()],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
        );

        assert_eq!(thread.classification, Classification::Internal);
        assert_eq!(thread.first_message_at, Some(expected_at));
        assert_eq!(thread.first_client_message_at, None);
    }

    #[test]
    fn internal_first_with_human_client_is_audited_not_dropped() {
        let thread = classify_thread(
            "run",
            "mixed",
            &[
                msg("m1", "agent@company.test", true, 0),
                msg("m2", "client@example.com", false, 10),
            ],
            &AnalysisConfig {
                date_from: "2026-06-01".to_string(),
                date_to: "2026-06-12".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec!["company.test".to_string()],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
        );

        // Internal-first no longer hard-drops to Misc: a human client exists, so it is
        // routed to the AI auditor (ambiguous + manual review) with the client id set.
        assert_eq!(thread.classification, Classification::Ambiguous);
        assert!(thread.manual_review_required);
        assert!(!thread.is_valid_client_request);
        assert_eq!(thread.first_client_message_id, Some("m2".to_string()));
    }

    #[test]
    fn gmail_promotions_label_routes_valid_thread_to_review() {
        let config = AnalysisConfig {
            date_from: "2026-06-01".to_string(),
            date_to: "2026-06-12".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["company.test".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        };
        let mut thread = classify_thread(
            "run",
            "promo",
            &[
                msg("m1", "client@example.com", false, 0),
                msg("m2", "agent@company.test", true, 25),
            ],
            &config,
        );
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_valid_client_request);

        refine_classification_with_folders(
            &mut thread,
            &["INBOX".to_string(), "CATEGORY_PROMOTIONS".to_string()],
        );
        assert_eq!(thread.classification, Classification::Newsletter);
        assert!(!thread.is_valid_client_request);
        assert!(thread.manual_review_required);
    }

    #[test]
    fn gmail_labels_do_not_touch_ai_applied_threads() {
        let config = AnalysisConfig {
            date_from: "2026-06-01".to_string(),
            date_to: "2026-06-12".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["company.test".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        };
        let mut thread = classify_thread(
            "run",
            "promo-ai",
            &[
                msg("m1", "client@example.com", false, 0),
                msg("m2", "agent@company.test", true, 25),
            ],
            &config,
        );
        thread.classification_source = ClassificationSource::Ai;
        refine_classification_with_folders(&mut thread, &["CATEGORY_PROMOTIONS".to_string()]);
        // La IA manda: no se reclasifica ni se fuerza revisión por la etiqueta.
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_valid_client_request);
    }

    #[test]
    fn internal_domain_matching_accepts_leading_at() {
        assert!(is_internal_email(
            "agente@west-ingenieria.cl",
            &["@west-ingenieria.cl".to_string()]
        ));
        assert!(is_internal_email(
            "agente@west-ingenieria.cl",
            &["west-ingenieria.cl".to_string()]
        ));
        assert!(!is_internal_email(
            "agente@otra-west-ingenieria.cl",
            &["@west-ingenieria.cl".to_string()]
        ));
    }

    #[test]
    fn auto_submitted_only_flags_canonical_auto_tokens() {
        let canonical = serde_json::json!({ "auto-submitted": "auto-generated" });
        assert!(is_automated_sender("agente@cliente.cl", &canonical));

        let replied = serde_json::json!({ "auto-submitted": "Auto-Replied" });
        assert!(is_automated_sender("agente@cliente.cl", &replied));

        // "no" y valores no canónicos no deben marcar el correo como automático.
        let explicit_no = serde_json::json!({ "auto-submitted": "no" });
        assert!(!is_automated_sender("persona@cliente.cl", &explicit_no));

        let non_canonical = serde_json::json!({ "auto-submitted": "yes" });
        assert!(!is_automated_sender("persona@cliente.cl", &non_canonical));
    }

    #[test]
    fn no_reply_local_parts_are_still_automated() {
        let headers = serde_json::json!({});
        assert!(is_automated_sender("no-reply@plataforma.cl", &headers));
        assert!(is_automated_sender("noreply@plataforma.cl", &headers));
        assert!(!is_automated_sender("soporte@plataforma.cl", &headers));
    }

    fn long_thread_config() -> AnalysisConfig {
        AnalysisConfig {
            date_from: "2026-06-01".to_string(),
            date_to: "2026-06-12".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["company.test".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        }
    }

    #[test]
    fn long_but_quickly_resolved_thread_is_not_suspicious() {
        // Cuatro mensajes externos (forma "larga") pero cerrado en 30 min: bien atendido.
        let thread = classify_thread(
            "run",
            "long-fast",
            &[
                msg("c1", "client@example.com", false, 0),
                msg("c2", "client@example.com", false, 5),
                msg("c3", "client@example.com", false, 10),
                msg("c4", "client@example.com", false, 15),
                msg("a1", "agent@company.test", true, 30),
            ],
            &long_thread_config(),
        );
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_valid_client_request);
        assert!(!thread.manual_review_required);
        assert_eq!(thread.classification_confidence, 0.86);
    }

    #[test]
    fn long_and_slowly_resolved_thread_stays_suspicious() {
        // Misma forma larga pero la respuesta interna llega 10 h después: caso atípico.
        let slow_reply_minutes = SUSPICIOUS_QUICK_RESOLUTION_MINUTES + 120;
        let thread = classify_thread(
            "run",
            "long-slow",
            &[
                msg("c1", "client@example.com", false, 0),
                msg("c2", "client@example.com", false, 5),
                msg("c3", "client@example.com", false, 10),
                msg("c4", "client@example.com", false, 15),
                msg("a1", "agent@company.test", true, slow_reply_minutes),
            ],
            &long_thread_config(),
        );
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_valid_client_request);
        assert!(thread.manual_review_required);
        assert_eq!(thread.classification_confidence, 0.72);
    }

    fn valid_thread_with_subject(subject: &str) -> EmailThread {
        let mut thread = classify_thread(
            "run",
            "hint",
            &[
                msg("m1", "client@example.com", false, 0),
                msg("m2", "agent@company.test", true, 20),
            ],
            &long_thread_config(),
        );
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(!thread.manual_review_required);
        thread.subject = subject.to_string();
        thread
    }

    #[test]
    fn policy_hint_routes_matching_valid_thread_to_review() {
        let rules = vec!["Soporte de hardware de impresoras".to_string()];
        let mut thread =
            valid_thread_with_subject("Falla de hardware en las impresoras del piso 3");
        refine_classification_with_policy_hints(&mut thread, "client@example.com", &rules);
        // No invierte la clasificación: solo la enruta a revisión manual.
        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_valid_client_request);
        assert!(thread.manual_review_required);
    }

    #[test]
    fn policy_hint_needs_two_distinct_terms_for_multiword_rule() {
        let rules = vec!["Soporte de hardware de impresoras".to_string()];
        // Solo coincide un término distintivo ("hardware"): no es coincidencia clara.
        let mut thread = valid_thread_with_subject("Consulta sobre el hardware contratado");
        refine_classification_with_policy_hints(&mut thread, "client@example.com", &rules);
        assert!(!thread.manual_review_required);
    }

    #[test]
    fn policy_hint_ignores_non_matching_thread() {
        let rules = vec!["Soporte de hardware de impresoras".to_string()];
        let mut thread = valid_thread_with_subject("Consulta general sobre el servicio contratado");
        refine_classification_with_policy_hints(&mut thread, "client@example.com", &rules);
        assert!(!thread.manual_review_required);
    }

    #[test]
    fn policy_hint_does_not_touch_ai_threads() {
        let rules = vec!["Soporte de hardware de impresoras".to_string()];
        let mut thread =
            valid_thread_with_subject("Falla de hardware en las impresoras del piso 3");
        thread.classification_source = ClassificationSource::Ai;
        refine_classification_with_policy_hints(&mut thread, "client@example.com", &rules);
        assert!(!thread.manual_review_required);
    }

    #[test]
    fn keyword_matches_uses_word_boundaries_for_single_words() {
        let kw = vec!["ticket".to_string()];
        assert!(keyword_matches("creamos el ticket #5", &kw));
        assert!(!keyword_matches("compramos una ticketera nueva", &kw));
    }

    #[test]
    fn keyword_matches_handles_accents_and_multiword() {
        assert!(keyword_matches(
            "registramos la incidencia del cliente",
            &["incidencia".to_string()],
        ));
        assert!(keyword_matches(
            "esto va a la mesa de ayuda",
            &["mesa de ayuda".to_string()],
        ));
        assert!(!keyword_matches("texto cualquiera", &["".to_string()]));
    }

    /// Mensaje externo automático (p. ej. un correo del cliente vía sistema): la
    /// heurística lo deja como `Automated` porque no hay cliente humano en ventana.
    fn automated_external_msg(id: &str, subject: &str, body: &str) -> EmailMessage {
        let mut message = msg(id, "sistema@example.com", false, 0);
        message.is_automated = true;
        message.subject = subject.to_string();
        message.body_text = Some(body.to_string());
        message
    }

    #[test]
    fn rescue_promotes_automated_thread_when_body_mentions_ticket() {
        let messages = [automated_external_msg(
            "m1",
            "Re: consulta",
            "Estimado, hemos creado el ticket #4521 para su solicitud.",
        )];
        let mut thread = classify_thread("run", "t", &messages, &long_thread_config());
        assert_eq!(thread.classification, Classification::Automated);
        assert!(!thread.is_valid_client_request);

        rescue_classification_with_valid_signals(&mut thread, &messages, &["ticket".to_string()]);

        assert_eq!(thread.classification, Classification::ValidClientRequest);
        assert!(thread.is_valid_client_request);
        assert!(thread.manual_review_required);
        assert_eq!(
            thread.classification_source,
            ClassificationSource::Heuristics
        );
        // Ancla fijada para que la auditoría IA evalúe el rescate.
        assert_eq!(thread.first_client_message_id.as_deref(), Some("m1"));
    }

    #[test]
    fn rescue_is_noop_without_keywords() {
        let messages = [automated_external_msg(
            "m1",
            "Re: consulta",
            "ticket creado",
        )];
        let mut thread = classify_thread("run", "t", &messages, &long_thread_config());
        let before = thread.classification.clone();
        rescue_classification_with_valid_signals(&mut thread, &messages, &[]);
        assert_eq!(thread.classification, before);
        assert!(!thread.is_valid_client_request);
    }

    #[test]
    fn rescue_does_not_override_ai_or_manual() {
        let messages = [automated_external_msg(
            "m1",
            "Re: consulta",
            "ticket creado",
        )];
        let mut thread = classify_thread("run", "t", &messages, &long_thread_config());
        thread.classification_source = ClassificationSource::Ai;
        rescue_classification_with_valid_signals(&mut thread, &messages, &["ticket".to_string()]);
        assert_eq!(thread.classification, Classification::Automated);
        assert!(!thread.is_valid_client_request);
    }

    #[test]
    fn rescue_leaves_already_valid_threads_untouched() {
        let mut thread = valid_thread_with_subject("Necesito ayuda con un ticket");
        let reasons_before = thread.reasons.len();
        rescue_classification_with_valid_signals(&mut thread, &[], &["ticket".to_string()]);
        // Ya era válido: sin cambios ni razones nuevas.
        assert_eq!(thread.reasons.len(), reasons_before);
    }
}
