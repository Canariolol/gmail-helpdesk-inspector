-- `equipo` fue retirado del catálogo antes de normalizar billing. Conserva el
-- acceso de suscripciones legacy asignándolas al plan vigente más alto.
INSERT INTO billing.subscriptions
    (id, org_id, plan_id, billing_interval, status, provider, provider_subscription_id,
     current_period_start, current_period_end, trial_ends_at, cancel_at_period_end,
     created_at, updated_at)
SELECT
    data->>'id',
    data->>'org_id',
    'pro',
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
WHERE kind = 'subscription' AND data->>'plan_id' = 'equipo'
ON CONFLICT (org_id) DO NOTHING;
