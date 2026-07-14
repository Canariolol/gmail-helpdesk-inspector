import type { AccountStatus, SubscriptionStatus } from "../../api/types";

/**
 * Estado de acceso derivado de la cuenta WorkOS + entitlement + conexión Gmail.
 *
 * Mapea los estados del flujo:
 *   landing -> crear cuenta -> plan/checkout -> trial/activo -> conectar Gmail -> app
 */
export type AccessState =
  | { kind: "pricing" }
  | { kind: "blocked"; status: SubscriptionStatus }
  | { kind: "connect_gmail" }
  | { kind: "ready" };

/** Estados de suscripción que bloquean el acceso pero mantienen la cuenta (pago requerido). */
const BLOCKED_STATUSES: readonly SubscriptionStatus[] = ["past_due", "cancelled", "expired"];

export function isBlockedStatus(status: SubscriptionStatus | null): boolean {
  return status !== null && BLOCKED_STATUSES.includes(status);
}

export function deriveAccessState(account: AccountStatus): AccessState {
  const entitlement = account.entitlement;

  if (entitlement.allowed) {
    return account.gmail_connected ? { kind: "ready" } : { kind: "connect_gmail" };
  }

  const status = entitlement.subscription_status;

  if (isBlockedStatus(status)) {
    return { kind: "blocked", status: status as SubscriptionStatus };
  }

  return { kind: "pricing" };
}
