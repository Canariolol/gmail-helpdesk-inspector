-- El catálogo refleja la capacidad disponible: una casilla y un miembro por
-- organización. No modifica suscripciones ni crea accesos nuevos.
UPDATE billing.plans
SET mailboxes = 1, members = 1, updated_at = now()
WHERE id IN ('gratis', 'inicial', 'pro')
  AND (mailboxes <> 1 OR members <> 1);

-- Permite recuperar una compra interrumpida por organización sin recorrer
-- todas las sesiones de otros tenants.
CREATE INDEX IF NOT EXISTS billing_incomplete_checkout_by_org
    ON billing.checkout_sessions (org_id, created_at DESC)
    WHERE status IN ('pending', 'provider_created');
