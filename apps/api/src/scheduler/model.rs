use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Configuración del análisis programado de un usuario. Vive en
/// `mira.records` con `kind='schedule_config'` e `id={user_email}`, y todos los
/// campos opcionales tienen default para que el JSON pueda editarse a mano sin
/// romper la deserialización.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub user_email: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub internal_domains: Vec<String>,
    #[serde(default)]
    pub ignored_senders: Vec<String>,
    #[serde(default)]
    pub ignored_domains: Vec<String>,
    #[serde(default)]
    pub ignored_keywords: Vec<String>,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_analysis_time")]
    pub analysis_time: String,
    #[serde(default)]
    pub gmail_max_threads: Option<u32>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleRunStatus {
    Running,
    Completed,
    Failed,
}

/// Estado del último intento programado por usuario (`scheduleStates/{email}`).
/// Sirve de candado de idempotencia: una ventana ya completada no se repite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleState {
    pub user_email: String,
    pub window_date_from: String,
    pub window_date_to: String,
    pub status: ScheduleRunStatus,
    pub run_id: Option<String>,
    pub email_sent: bool,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

fn default_timezone() -> String {
    "America/Santiago".to_string()
}

fn default_analysis_time() -> String {
    "08:00".to_string()
}
