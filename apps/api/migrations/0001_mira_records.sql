CREATE SCHEMA IF NOT EXISTS mira;

REVOKE ALL ON SCHEMA mira FROM PUBLIC, anon, authenticated, service_role;

CREATE TABLE IF NOT EXISTS mira.records (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    owner_email TEXT,
    workos_user_id TEXT,
    workos_session_id TEXT,
    org_id TEXT,
    run_id TEXT,
    thread_id TEXT,
    provider TEXT,
    provider_id TEXT,
    state TEXT,
    sort_at TIMESTAMPTZ,
    data JSONB NOT NULL CHECK (jsonb_typeof(data) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, id)
);

CREATE UNIQUE INDEX IF NOT EXISTS records_account_email_unique
    ON mira.records (owner_email)
    WHERE kind = 'account';

CREATE INDEX IF NOT EXISTS records_workos_user_lookup
    ON mira.records (kind, workos_user_id)
    WHERE workos_user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS records_workos_session_lookup
    ON mira.records (kind, workos_session_id)
    WHERE workos_session_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS records_org_unique
    ON mira.records (kind, org_id)
    WHERE kind IN ('subscription', 'organization');

CREATE UNIQUE INDEX IF NOT EXISTS records_provider_lookup
    ON mira.records (kind, provider, provider_id)
    WHERE provider_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS records_owner_kind_sort
    ON mira.records (owner_email, kind, sort_at DESC)
    WHERE owner_email IS NOT NULL;

CREATE INDEX IF NOT EXISTS records_run_kind_sort
    ON mira.records (run_id, kind, sort_at)
    WHERE run_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS records_thread_lookup
    ON mira.records (thread_id, kind)
    WHERE thread_id IS NOT NULL;

ALTER TABLE mira.records ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON mira.records FROM PUBLIC, anon, authenticated, service_role;
