-- Normaliza subscription/checkout_session/usage_ledger, que hoy viven como
-- filas JSONB en mira.records (kind IN ('subscription','checkout','usage_ledger')),
-- en un schema `billing` propio con tablas y columnas reales. Los precios y
-- límites de plan siguen viviendo en apps/api/src/billing.rs (fuente de verdad,
-- protegida por tests); billing.plans es un espejo sembrado para integridad
-- referencial (FK) y consultas de ops, no se lee en caliente para servir precios.
--
-- Las filas viejas en mira.records NO se borran acá: quedan inertes como red de
-- seguridad hasta una migración de limpieza posterior, una vez verificado en
-- producción. Antes de aplicar en producción, correr scripts/backup-postgres.sh.

CREATE SCHEMA IF NOT EXISTS billing;
REVOKE ALL ON SCHEMA billing FROM PUBLIC, anon, authenticated, service_role;

CREATE TABLE IF NOT EXISTS billing.plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    clp_monthly INTEGER NOT NULL,
    clp_annual INTEGER,
    usd_reference_monthly INTEGER NOT NULL,
    trial_days INTEGER NOT NULL DEFAULT 0,
    highlighted BOOLEAN NOT NULL DEFAULT false,
    mailboxes INTEGER NOT NULL,
    members INTEGER NOT NULL,
    runs_per_month INTEGER NOT NULL,
    retrieved_threads_per_month INTEGER NOT NULL,
    reported_threads_per_run INTEGER NOT NULL,
    ai_analyzed_threads_per_month INTEGER NOT NULL,
    report_recipients INTEGER NOT NULL,
    retention_days INTEGER NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO billing.plans
    (id, name, clp_monthly, clp_annual, usd_reference_monthly, trial_days, highlighted,
     mailboxes, members, runs_per_month, retrieved_threads_per_month, reported_threads_per_run,
     ai_analyzed_threads_per_month, report_recipients, retention_days)
VALUES
    ('gratis', 'Mira Free', 0, NULL, 0, 0, false,
     1, 1, 10, 400, 40, 100, 1, 14),
    ('inicial', 'Inicial', 9990, 99900, 9, 0, false,
     1, 1, 40, 4000, 2147483647, 800, 3, 30),
    ('pro', 'Pro', 29990, 299900, 29, 30, true,
     3, 3, 120, 15000, 2147483647, 3000, 10, 90)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS billing.subscriptions (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL UNIQUE,
    plan_id TEXT NOT NULL REFERENCES billing.plans(id),
    billing_interval TEXT NOT NULL DEFAULT 'monthly',
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_subscription_id TEXT,
    current_period_start TIMESTAMPTZ,
    current_period_end TIMESTAMPTZ,
    trial_ends_at TIMESTAMPTZ,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS billing_subscriptions_provider_lookup
    ON billing.subscriptions (provider, provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS billing.checkout_sessions (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    account_email TEXT NOT NULL,
    plan_id TEXT NOT NULL REFERENCES billing.plans(id),
    billing_interval TEXT NOT NULL DEFAULT 'monthly',
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_subscription_id TEXT,
    currency_id TEXT NOT NULL,
    amount_clp INTEGER NOT NULL,
    usd_reference_monthly INTEGER NOT NULL,
    trial_days INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS billing_checkout_sessions_provider_lookup
    ON billing.checkout_sessions (provider, provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS billing.usage_ledger (
    org_id TEXT NOT NULL,
    period_key TEXT NOT NULL,
    runs_created BIGINT NOT NULL DEFAULT 0 CHECK (runs_created BETWEEN 0 AND 4294967295),
    retrieved_threads BIGINT NOT NULL DEFAULT 0 CHECK (retrieved_threads BETWEEN 0 AND 4294967295),
    ai_analyzed_threads BIGINT NOT NULL DEFAULT 0 CHECK (ai_analyzed_threads BETWEEN 0 AND 4294967295),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, period_key)
);

-- Dedup de avisos de consumo 80%/100% (sección 8 del plan comercial).
CREATE TABLE IF NOT EXISTS billing.quota_alerts (
    org_id TEXT NOT NULL,
    period_key TEXT NOT NULL,
    axis TEXT NOT NULL,
    threshold SMALLINT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, period_key, axis, threshold)
);

ALTER TABLE billing.plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing.subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing.checkout_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing.usage_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing.quota_alerts ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON ALL TABLES IN SCHEMA billing FROM PUBLIC, anon, authenticated, service_role;

-- Copia idempotente de los datos existentes desde mira.records. plan_id
-- inválido o desconocido se descarta (NULLIF + WHERE) en vez de romper la
-- migración; esas filas quedarían igual disponibles en mira.records para
-- inspección manual.
INSERT INTO billing.subscriptions
    (id, org_id, plan_id, billing_interval, status, provider, provider_subscription_id,
     current_period_start, current_period_end, trial_ends_at, cancel_at_period_end,
     created_at, updated_at)
SELECT
    data->>'id',
    data->>'org_id',
    data->>'plan_id',
    COALESCE(data->>'billing_interval', 'monthly'),
    data->>'status',
    data->>'provider',
    data->>'provider_subscription_id',
    (data->>'current_period_start')::timestamptz,
    (data->>'current_period_end')::timestamptz,
    (data->>'trial_ends_at')::timestamptz,
    COALESCE((data->>'cancel_at_period_end')::boolean, false),
    (data->>'created_at')::timestamptz,
    (data->>'updated_at')::timestamptz
FROM mira.records
WHERE kind = 'subscription'
  AND (data->>'plan_id') IN (SELECT id FROM billing.plans)
ON CONFLICT (org_id) DO NOTHING;

INSERT INTO billing.checkout_sessions
    (id, org_id, account_email, plan_id, billing_interval, status, provider,
     provider_subscription_id, currency_id, amount_clp, usd_reference_monthly,
     trial_days, created_at, updated_at)
SELECT
    data->>'id',
    data->>'org_id',
    data->>'account_email',
    data->>'plan_id',
    COALESCE(data->>'billing_interval', 'monthly'),
    data->>'status',
    data->>'provider',
    data->>'provider_subscription_id',
    data->>'currency_id',
    (data->>'amount_clp')::integer,
    (data->>'usd_reference_monthly')::integer,
    (data->>'trial_days')::integer,
    (data->>'created_at')::timestamptz,
    (data->>'updated_at')::timestamptz
FROM mira.records
WHERE kind = 'checkout'
  AND (data->>'plan_id') IN (SELECT id FROM billing.plans)
ON CONFLICT (id) DO NOTHING;

INSERT INTO billing.usage_ledger
    (org_id, period_key, runs_created, retrieved_threads, ai_analyzed_threads, updated_at)
SELECT
    data->>'org_id',
    data->>'period_key',
    COALESCE((data->>'runs_created')::bigint, 0),
    COALESCE(
        (data->>'retrieved_threads')::bigint,
        (data->>'candidate_threads')::bigint,
        (data->>'analyzed_threads')::bigint,
        0
    ),
    COALESCE(
        (data->>'ai_analyzed_threads')::bigint,
        (data->>'ai_audited_threads')::bigint,
        0
    ),
    (data->>'updated_at')::timestamptz
FROM mira.records
WHERE kind = 'usage_ledger'
ON CONFLICT (org_id, period_key) DO NOTHING;
