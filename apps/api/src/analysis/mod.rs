use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const AI_AUTO_APPLY_THRESHOLD: f64 = 0.92;

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisMetrics {
    pub total_threads: u64,
    pub valid_requests: u64,
    pub answered: u64,
    pub unanswered: u64,
    pub ignored: u64,
    pub ambiguous: u64,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRun {
    pub id: String,
    pub user_email: String,
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
    pub gmail_message_id: String,
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
    pub gmail_thread_id: String,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAuditResult {
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
    gmail_thread_id: &str,
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
            gmail_thread_id,
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

    if first_message.is_internal {
        reasons.push(
            "El hilo comenzó con un correo interno; se excluye por ser saliente.".to_string(),
        );
        return build_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Classification::Misc,
            ClassificationSource::Rules,
            0.96,
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
    }

    if first_message.is_automated {
        reasons.push("El hilo comenzó con un remitente externo automático.".to_string());
        return build_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Classification::Automated,
            ClassificationSource::Rules,
            0.95,
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
    }

    if config
        .ignored_keywords
        .iter()
        .any(|kw| lower_subject.contains(&kw.to_lowercase()))
    {
        reasons.push("El asunto coincide con una palabra ignorada.".to_string());
        return ignored_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Some(first_message.date),
            reasons,
            now,
        );
    }

    if messages.iter().all(|m| m.is_internal) {
        reasons.push("Todos los participantes son internos.".to_string());
        return build_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Classification::Internal,
            ClassificationSource::Rules,
            0.98,
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
    }

    if messages.iter().any(|m| m.is_automated)
        && messages.iter().all(|m| !m.is_external || m.is_automated)
    {
        reasons.push("Los mensajes externos parecen automáticos.".to_string());
        return build_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Classification::Automated,
            ClassificationSource::Rules,
            0.95,
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
    }

    if lower_subject.contains("newsletter")
        || lower_subject.contains("boletin")
        || lower_subject.contains("boletín")
    {
        reasons.push("El asunto parece un boletín o correo promocional.".to_string());
        return build_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Classification::Newsletter,
            ClassificationSource::Rules,
            0.92,
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
    }

    if messages.iter().any(|m| {
        sender_is_ignored(
            &m.from_email,
            &config.ignored_senders,
            &config.ignored_domains,
        )
    }) {
        reasons.push("El remitente o dominio está configurado como ignorado.".to_string());
        return ignored_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Some(first_message.date),
            reasons,
            now,
        );
    }

    let first_client = messages
        .iter()
        .filter(|m| m.is_external && !m.is_automated)
        .min_by_key(|m| m.date);

    let Some(first_client) = first_client else {
        reasons.push("No se encontró un remitente externo humano claro.".to_string());
        return build_thread(
            analysis_run_id,
            gmail_thread_id,
            subject,
            normalized_subject,
            Classification::Ambiguous,
            ClassificationSource::Heuristics,
            0.45,
            false,
            false,
            None,
            None,
            None,
            Some(first_message.date),
            true,
            reasons,
            now,
        );
    };

    let first_reply = messages
        .iter()
        .filter(|m| m.is_internal && !m.is_automated && m.date > first_client.date)
        .min_by_key(|m| m.date);
    let last_internal = messages
        .iter()
        .filter(|m| m.is_internal && !m.is_automated && m.date > first_client.date)
        .max_by_key(|m| m.date);
    let response_minutes = first_reply.map(|reply| (reply.date - first_client.date).num_minutes());
    let resolution_minutes =
        last_internal.map(|message| (message.date - first_client.date).num_minutes());
    let suspicious = messages.len() >= 8 || messages.iter().filter(|m| m.is_external).count() >= 4;

    reasons.push("El primer mensaje relevante viene de un remitente externo humano.".to_string());
    if first_reply.is_some() {
        reasons.push("Se encontró una respuesta interna posterior no automática.".to_string());
    } else {
        reasons.push("No se encontró una respuesta interna posterior.".to_string());
    }
    if suspicious {
        reasons.push("La forma del hilo es sospechosa y requiere auditoría.".to_string());
    }

    build_thread(
        analysis_run_id,
        gmail_thread_id,
        subject,
        normalized_subject,
        Classification::ValidClientRequest,
        ClassificationSource::Heuristics,
        if suspicious { 0.72 } else { 0.86 },
        true,
        first_reply.is_some(),
        Some(first_client.id.clone()),
        first_reply.map(|m| m.id.clone()),
        last_internal.map(|m| m.id.clone()),
        Some(first_message.date),
        suspicious,
        reasons,
        now,
    )
    .with_dates(
        Some(first_client.date),
        first_reply.map(|m| m.date),
        last_internal.map(|m| m.date),
        response_minutes,
        resolution_minutes,
    )
}

fn ignored_thread(
    analysis_run_id: &str,
    gmail_thread_id: &str,
    subject: String,
    normalized_subject: String,
    first_message_at: Option<DateTime<Utc>>,
    reasons: Vec<String>,
    now: DateTime<Utc>,
) -> EmailThread {
    build_thread(
        analysis_run_id,
        gmail_thread_id,
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
    gmail_thread_id: &str,
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
        id: gmail_thread_id.to_string(),
        analysis_run_id: analysis_run_id.to_string(),
        gmail_thread_id: gmail_thread_id.to_string(),
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
    let lower = email.to_lowercase();
    lower.contains("no-reply")
        || lower.contains("noreply")
        || lower.contains("notification")
        || lower.contains("mailer-daemon")
        || headers
            .get("auto-submitted")
            .and_then(|v| v.as_str())
            .map(|value| value.to_lowercase() != "no")
            .unwrap_or(false)
        || headers.get("list-unsubscribe").is_some()
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
        .filter(|t| t.is_valid_client_request)
        .collect();
    let valid_requests = valid.len() as u64;
    let answered = valid.iter().filter(|t| t.is_answered).count() as u64;
    let ambiguous = threads
        .iter()
        .filter(|t| t.classification == Classification::Ambiguous || t.manual_review_required)
        .count() as u64;
    let ignored = threads
        .iter()
        .filter(|t| !t.is_valid_client_request && t.classification != Classification::Ambiguous)
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
        (1.0 - (ambiguous as f64 / total_threads as f64)).clamp(0.0, 1.0)
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

pub fn should_auto_apply_ai(result: &AiAuditResult, known_message_ids: &[String]) -> bool {
    if result.confidence < AI_AUTO_APPLY_THRESHOLD || result.manual_review_required {
        return false;
    }
    let first_client_ok = result
        .first_client_message_id
        .as_ref()
        .map(|id| known_message_ids.contains(id))
        .unwrap_or(true);
    let first_reply_ok = result
        .first_internal_reply_message_id
        .as_ref()
        .map(|id| known_message_ids.contains(id))
        .unwrap_or(true);
    let last_internal_ok = result
        .last_internal_message_id
        .as_ref()
        .map(|id| known_message_ids.contains(id))
        .unwrap_or(true);
    first_client_ok && first_reply_ok && last_internal_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, from: &str, internal: bool, at_minute: i64) -> EmailMessage {
        EmailMessage {
            id: id.to_string(),
            gmail_message_id: id.to_string(),
            from_email: from.to_string(),
            from_name: None,
            to_emails: vec![],
            cc_emails: vec![],
            date: DateTime::from_timestamp(1_700_000_000 + at_minute * 60, 0).unwrap(),
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
            },
        );

        assert_eq!(thread.classification, Classification::Misc);
        assert_eq!(thread.first_message_at, Some(expected_at));
        assert_eq!(thread.first_client_message_at, None);
    }

    #[test]
    fn ai_auto_apply_requires_threshold_and_known_ids() {
        let result = AiAuditResult {
            classification: Classification::ValidClientRequest,
            is_valid_client_request: true,
            is_answered: true,
            first_client_message_id: Some("m1".to_string()),
            first_internal_reply_message_id: Some("m2".to_string()),
            last_internal_message_id: Some("m2".to_string()),
            confidence: 0.93,
            manual_review_required: false,
            issues: vec![],
            input_tokens: 10,
            output_tokens: 5,
        };
        assert!(should_auto_apply_ai(
            &result,
            &["m1".to_string(), "m2".to_string()]
        ));
        assert!(!should_auto_apply_ai(&result, &["m1".to_string()]));
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
}
