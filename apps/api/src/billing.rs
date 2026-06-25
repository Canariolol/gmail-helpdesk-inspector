use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BillingPlanId {
    Gratis,
    Inicial,
    Pro,
    Equipo,
}

impl BillingPlanId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gratis => "gratis",
            Self::Inicial => "inicial",
            Self::Pro => "pro",
            Self::Equipo => "equipo",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gratis" => Some(Self::Gratis),
            "inicial" => Some(Self::Inicial),
            "pro" => Some(Self::Pro),
            "equipo" => Some(Self::Equipo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPlan {
    pub id: BillingPlanId,
    pub name: String,
    pub usd_reference_monthly: u32,
    pub clp_monthly: u32,
    pub trial_days: u32,
    pub limits: PlanLimits,
    pub highlighted: bool,
}

/// Centinela para "sin tope de hilos analizados por análisis" (planes de pago):
/// `analyzed_threads_per_run` con este valor significa que el plan no limita el run
/// (el límite real lo pone la policy de recuperación y el cupo mensual).
pub const UNLIMITED_ANALYZED_PER_RUN: u32 = u32::MAX;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanLimits {
    pub mailboxes: u32,
    pub members: u32,
    pub runs_per_month: u32,
    /// Cupo mensual de hilos efectivamente ANALIZADOS (los que sobreviven el embudo
    /// y se guardan), no los recuperados de Gmail.
    pub analyzed_threads_per_month: u32,
    /// Tope de hilos ANALIZADOS por cada análisis. `UNLIMITED_ANALYZED_PER_RUN` = sin tope.
    pub analyzed_threads_per_run: u32,
    pub ai_audited_threads_per_month: u32,
    pub report_recipients: u32,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub workos_user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub org_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Pending,
    Trialing,
    Active,
    PastDue,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub org_id: String,
    pub plan_id: BillingPlanId,
    pub status: SubscriptionStatus,
    pub provider: String,
    pub provider_subscription_id: Option<String>,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub trial_ends_at: Option<DateTime<Utc>>,
    /// Cancelación agendada: corta cobros pero mantiene acceso hasta el fin del período pagado.
    #[serde(default)]
    pub cancel_at_period_end: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutSessionStatus {
    Pending,
    ProviderCreated,
    Activated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub id: String,
    pub org_id: String,
    pub account_email: String,
    pub plan_id: BillingPlanId,
    pub status: CheckoutSessionStatus,
    pub provider: String,
    pub provider_subscription_id: Option<String>,
    pub checkout_url: Option<String>,
    pub currency_id: String,
    pub amount_clp: u32,
    pub usd_reference_monthly: u32,
    pub trial_days: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLedger {
    pub org_id: String,
    pub period_key: String,
    pub runs_created: u32,
    /// Hilos efectivamente ANALIZADOS (guardados) en el período, no los recuperados.
    #[serde(default, alias = "candidate_threads")]
    pub analyzed_threads: u32,
    pub ai_audited_threads: u32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementSnapshot {
    pub allowed: bool,
    pub reason: Option<String>,
    pub subscription_status: Option<SubscriptionStatus>,
    pub plan: Option<BillingPlan>,
    #[serde(default)]
    pub cancel_at_period_end: bool,
    pub current_period_end: Option<DateTime<Utc>>,
    pub trial_ends_at: Option<DateTime<Utc>>,
}

pub fn public_plans() -> Vec<BillingPlan> {
    vec![
        BillingPlan {
            id: BillingPlanId::Inicial,
            name: "Inicial".to_string(),
            usd_reference_monthly: 9,
            clp_monthly: 9_990,
            trial_days: 0,
            highlighted: false,
            limits: PlanLimits {
                mailboxes: 1,
                members: 1,
                runs_per_month: 10,
                analyzed_threads_per_month: 1_000,
                analyzed_threads_per_run: UNLIMITED_ANALYZED_PER_RUN,
                ai_audited_threads_per_month: 250,
                report_recipients: 3,
                retention_days: 30,
            },
        },
        BillingPlan {
            id: BillingPlanId::Pro,
            name: "Pro".to_string(),
            usd_reference_monthly: 29,
            clp_monthly: 29_990,
            trial_days: 30,
            highlighted: true,
            limits: PlanLimits {
                mailboxes: 2,
                members: 3,
                runs_per_month: 50,
                analyzed_threads_per_month: 7_500,
                analyzed_threads_per_run: UNLIMITED_ANALYZED_PER_RUN,
                ai_audited_threads_per_month: 2_000,
                report_recipients: 10,
                retention_days: 90,
            },
        },
        BillingPlan {
            id: BillingPlanId::Equipo,
            name: "Equipo".to_string(),
            usd_reference_monthly: 99,
            clp_monthly: 99_990,
            trial_days: 0,
            highlighted: false,
            limits: PlanLimits {
                mailboxes: 5,
                members: 10,
                runs_per_month: 200,
                analyzed_threads_per_month: 25_000,
                analyzed_threads_per_run: UNLIMITED_ANALYZED_PER_RUN,
                ai_audited_threads_per_month: 8_000,
                report_recipients: 25,
                retention_days: 180,
            },
        },
    ]
}

/// Plan gratuito por defecto (sin tarjeta). No es comprable: se asigna como plan
/// vigente cuando una org no tiene suscripción que dé acceso (nueva o churned).
/// Límites mini, todos tuneables aquí. El cupo IA permite que cualquier hilo
/// analizado pueda entrar al embudo batch, sin obligar a auditar todos individualmente.
pub fn free_plan() -> BillingPlan {
    BillingPlan {
        id: BillingPlanId::Gratis,
        name: "Mira Free".to_string(),
        usd_reference_monthly: 0,
        clp_monthly: 0,
        trial_days: 0,
        highlighted: false,
        limits: PlanLimits {
            mailboxes: 1,
            members: 1,
            // Backstop anti-abuso; el límite real es el cupo de hilos analizados.
            runs_per_month: 15,
            analyzed_threads_per_month: 120,
            analyzed_threads_per_run: 40,
            ai_audited_threads_per_month: 120,
            report_recipients: 1,
            retention_days: 14,
        },
    }
}

pub fn plan_by_id(plan_id: &BillingPlanId) -> BillingPlan {
    if matches!(plan_id, BillingPlanId::Gratis) {
        return free_plan();
    }
    public_plans()
        .into_iter()
        .find(|plan| &plan.id == plan_id)
        .expect("known plan id")
}

pub fn subscription_allows_access(subscription: Option<&Subscription>, now: DateTime<Utc>) -> bool {
    let Some(subscription) = subscription else {
        return false;
    };
    match subscription.status {
        SubscriptionStatus::Active => {
            if subscription.cancel_at_period_end {
                // Cancelación agendada: acceso solo hasta que termina el período pagado.
                subscription.current_period_end.is_none_or(|end| end > now)
            } else {
                true
            }
        }
        SubscriptionStatus::Trialing => subscription.trial_ends_at.is_none_or(|ends| ends > now),
        _ => false,
    }
}

pub fn active_subscription_for_trial(
    org_id: String,
    plan_id: BillingPlanId,
    provider_subscription_id: Option<String>,
    now: DateTime<Utc>,
) -> Subscription {
    let plan = plan_by_id(&plan_id);
    Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        org_id,
        plan_id,
        status: if plan.trial_days > 0 {
            SubscriptionStatus::Trialing
        } else {
            SubscriptionStatus::Pending
        },
        provider: "mercadopago".to_string(),
        provider_subscription_id,
        current_period_start: Some(now),
        current_period_end: Some(now + Duration::days(30)),
        trial_ends_at: (plan.trial_days > 0)
            .then_some(now + Duration::days(plan.trial_days as i64)),
        cancel_at_period_end: false,
        created_at: now,
        updated_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(status: SubscriptionStatus, now: DateTime<Utc>) -> Subscription {
        Subscription {
            id: "sub-1".to_string(),
            org_id: "org-1".to_string(),
            plan_id: BillingPlanId::Pro,
            status,
            provider: "mercadopago".to_string(),
            provider_subscription_id: Some("pre-1".to_string()),
            current_period_start: Some(now),
            current_period_end: Some(now + Duration::days(30)),
            trial_ends_at: None,
            cancel_at_period_end: false,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn active_subscription_allows_access() {
        let now = Utc::now();
        let sub = subscription(SubscriptionStatus::Active, now);
        assert!(subscription_allows_access(Some(&sub), now));
    }

    #[test]
    fn cancel_at_period_end_keeps_access_until_period_end() {
        let now = Utc::now();
        let mut sub = subscription(SubscriptionStatus::Active, now);
        sub.cancel_at_period_end = true;
        sub.current_period_end = Some(now + Duration::days(5));
        assert!(subscription_allows_access(Some(&sub), now));
    }

    #[test]
    fn cancel_at_period_end_blocks_after_period_end() {
        let now = Utc::now();
        let mut sub = subscription(SubscriptionStatus::Active, now);
        sub.cancel_at_period_end = true;
        sub.current_period_end = Some(now - Duration::days(1));
        assert!(!subscription_allows_access(Some(&sub), now));
    }

    #[test]
    fn expired_trial_blocks_access() {
        let now = Utc::now();
        let mut sub = subscription(SubscriptionStatus::Trialing, now);
        sub.trial_ends_at = Some(now - Duration::minutes(1));
        assert!(!subscription_allows_access(Some(&sub), now));
    }

    #[test]
    fn missing_subscription_blocks_access() {
        assert!(!subscription_allows_access(None, Utc::now()));
    }

    #[test]
    fn plan_by_id_resolves_free_without_panicking() {
        let plan = plan_by_id(&BillingPlanId::Gratis);
        assert_eq!(plan.id, BillingPlanId::Gratis);
        assert_eq!(plan.clp_monthly, 0);
    }

    #[test]
    fn free_plan_caps_analyzed_and_has_matching_ai_eligibility() {
        let limits = free_plan().limits;
        assert_eq!(limits.analyzed_threads_per_run, 40);
        assert_eq!(limits.analyzed_threads_per_month, 120);
        // Todo hilo analizado puede ser elegible para el batch sin exceder el cupo.
        assert_eq!(
            limits.ai_audited_threads_per_month,
            limits.analyzed_threads_per_month
        );
    }

    #[test]
    fn paid_plans_have_no_per_run_analyzed_cap() {
        for plan in public_plans() {
            assert_eq!(
                plan.limits.analyzed_threads_per_run, UNLIMITED_ANALYZED_PER_RUN,
                "{} no debe topar hilos por análisis",
                plan.name
            );
        }
    }
}
