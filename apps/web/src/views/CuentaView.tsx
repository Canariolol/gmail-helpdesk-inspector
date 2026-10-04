import { ArrowUpRight, Ban, Check, LogOut, RefreshCw, ShieldAlert } from "lucide-react";
import { useState } from "react";
import type {
  AccountStatus,
  BillingInterval,
  BillingPlan,
  BillingPlanId,
  MailboxProviderId,
  SubscriptionStatus,
} from "../api/types";
import { ThemePicker } from "../components/common/ThemePicker";
import { isBlockedStatus } from "./access/accessState";
import { PaymentNotes } from "./access/PaymentNotes";
import { PricingPlans } from "./access/PricingPlans";

const PROVIDER_NAMES: Record<MailboxProviderId, string> = {
  google: "de Google",
  microsoft: "de Microsoft",
  imap: "de un proveedor IMAP",
};

interface CuentaViewProps {
  account: AccountStatus;
  plans: BillingPlan[];
  /** Checkout nuevo (reactivar plan bloqueado o reanudar tras cancelar). */
  onChoosePlan: (planId: BillingPlanId, billingInterval: BillingInterval) => void;
  checkoutLoadingPlanId: string | null;
  /** Abre el modal de cambio de plan (suscripción activa). */
  onOpenChangePlan: () => void;
  /** Cancela al fin del período. */
  onCancel: () => void;
  cancelPending: boolean;
  onReconcile: () => void;
  reconcilePending: boolean;
  onDisconnectGmail: () => void;
  gmailDisconnectPending: boolean;
  onLogoutAll: () => void;
  logoutAllPending: boolean;
  error: string | null;
}

const STATUS_LABELS: Record<SubscriptionStatus, string> = {
  pending: "Pendiente",
  trialing: "En prueba",
  active: "Activo",
  past_due: "Pago pendiente",
  cancelled: "Cancelada",
  expired: "Vencida",
};

const BLOCKED_COPY: Record<string, { title: string; body: string }> = {
  past_due: {
    title: "No pudimos confirmar tu último pago",
    body: "Tu plan quedó en pausa. Reactívalo para volver a generar y ver análisis.",
  },
  cancelled: {
    title: "Tu suscripción está cancelada",
    body: "Reactiva un plan cuando quieras para retomar tu auditoría.",
  },
  expired: {
    title: "Tu plan venció",
    body: "Tu prueba o período de pago terminó. Elige un plan para seguir auditando tu casilla.",
  },
};

function formatDate(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat("es-CL", { dateStyle: "long" }).format(date);
}

export function CuentaView({
  account,
  plans,
  onChoosePlan,
  checkoutLoadingPlanId,
  onOpenChangePlan,
  onCancel,
  cancelPending,
  onReconcile,
  reconcilePending,
  onDisconnectGmail,
  gmailDisconnectPending,
  onLogoutAll,
  logoutAllPending,
  error,
}: CuentaViewProps) {
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  const [confirmingGmailDisconnect, setConfirmingGmailDisconnect] = useState(false);
  const [confirmingLogoutAll, setConfirmingLogoutAll] = useState(false);
  const entitlement = account.entitlement;
  const planName = entitlement.plan?.name ?? null;
  const status = entitlement.subscription_status;
  const blocked = !entitlement.allowed && isBlockedStatus(status);
  const scheduledCancel = entitlement.cancel_at_period_end && entitlement.allowed;
  const canManageSubscription = status === "active" || status === "trialing";
  const canCancelSubscription = !scheduledCancel && (canManageSubscription || status === "past_due");

  const periodEnd = formatDate(entitlement.current_period_end);
  const trialEnd = formatDate(entitlement.trial_ends_at);
  // El acceso de una prueba termina en trial_ends_at; el de un plan pagado, en current_period_end.
  const accessEnd = status === "trialing" ? (trialEnd ?? periodEnd) : periodEnd;

  return (
    <div className="view cuenta-view">
      <section className="card">
        <div className="cuenta-head">
          <div>
            <span className="wizard-step-desc">Cuenta</span>
            <h2>{account.account_email}</h2>
          </div>
          {status && (
            <span className={`access-badge ${blocked ? "access-badge-warn" : "access-badge-ok"}`}>
              {blocked ? <ShieldAlert size={14} /> : <Check size={14} />}
              {STATUS_LABELS[status]}
            </span>
          )}
        </div>

        {error && (
          <div className="access-error" role="alert">
            {error}
          </div>
        )}

        {status && <button type="button" className="btn-ghost" disabled={reconcilePending} onClick={onReconcile}>
          <RefreshCw size={16} />{reconcilePending ? "Consultando pago…" : "Actualizar estado de pago"}
        </button>}

        {!blocked && (
          <div className="cuenta-plan">
            <p className="cuenta-plan-name">
              Plan <strong>{planName ?? "—"}</strong>
            </p>
            {scheduledCancel ? (
              <p className="cuenta-note-warn">
                <ShieldAlert size={15} /> Tu suscripción se cancela
                {accessEnd ? ` el ${accessEnd}` : " al final del período"}. Mantienes acceso hasta
                entonces.
              </p>
            ) : status === "trialing" && trialEnd ? (
              <p className="muted-note">Tu prueba termina el {trialEnd}.</p>
            ) : periodEnd ? (
              <p className="muted-note">Se renueva el {periodEnd}.</p>
            ) : null}

            <div className="cuenta-actions">
              {scheduledCancel ? (
                <p className="muted-note">Podrás contratar nuevamente al terminar el período que ya pagaste.</p>
              ) : canManageSubscription ? (
                  <button type="button" className="btn-primary" onClick={onOpenChangePlan}>
                    Cambiar de plan <ArrowUpRight size={16} />
                  </button>
              ) : status !== "past_due" ? (
                <button type="button" className="btn-primary" onClick={onOpenChangePlan}>
                  Elegir un plan <ArrowUpRight size={16} />
                </button>
              ) : (
                <p className="muted-note">Actualiza el estado de pago o cancela la suscripción anterior antes de contratar otra.</p>
              )}
            </div>
          </div>
        )}

        {canCancelSubscription && <div className="cuenta-actions">
                  {!confirmingCancel ? (
                    <button
                      type="button"
                      className="btn-ghost"
                      onClick={() => setConfirmingCancel(true)}
                    >
                      <Ban size={16} /> Cancelar suscripción
                    </button>
                  ) : (
                    <div className="cuenta-confirm">
                      <span>
                        Detendrás los próximos cobros. {entitlement.allowed && accessEnd ? `Mantendrás el acceso adquirido hasta el ${accessEnd}.` : "El acceso posterior se regirá por el plan disponible en tu cuenta."}
                        ¿Cancelar?
                      </span>
                      <div className="cuenta-confirm-actions">
                        <button
                          type="button"
                          className="btn-ghost"
                          disabled={cancelPending}
                          onClick={() => {
                            onCancel();
                            setConfirmingCancel(false);
                          }}
                        >
                          {cancelPending ? "Cancelando…" : "Sí, cancelar"}
                        </button>
                        <button
                          type="button"
                          className="access-text-btn"
                          onClick={() => setConfirmingCancel(false)}
                        >
                          Volver
                        </button>
                      </div>
                    </div>
                  )}
        </div>}

        {account.gmail_connected && (
          <div className="cuenta-plan cuenta-row">
            <p className="cuenta-plan-name">
              {account.mailbox_provider
                ? `Casilla ${PROVIDER_NAMES[account.mailbox_provider]} conectada`
                : "Casilla conectada"}{" "}
              <strong>{account.gmail_account_email}</strong>
            </p>
            {!confirmingGmailDisconnect ? (
              <button
                type="button"
                className="btn-ghost"
                onClick={() => setConfirmingGmailDisconnect(true)}
              >
                <Ban size={16} /> Desconectar casilla
              </button>
            ) : (
              <div className="cuenta-confirm">
                <span>Dejarás de analizar esta casilla. Los análisis existentes se conservan. ¿Desconectar?</span>
                <div className="cuenta-confirm-actions">
                  <button
                    type="button"
                    className="btn-ghost"
                    disabled={gmailDisconnectPending}
                    onClick={() => {
                      onDisconnectGmail();
                      setConfirmingGmailDisconnect(false);
                    }}
                  >
                    {gmailDisconnectPending ? "Desconectando…" : "Sí, desconectar"}
                  </button>
                  <button
                    type="button"
                    className="access-text-btn"
                    onClick={() => setConfirmingGmailDisconnect(false)}
                  >
                    Volver
                  </button>
                </div>
              </div>
            )}
          </div>
        )}

        <div className="cuenta-plan cuenta-row">
          <p className="cuenta-plan-name">Apariencia</p>
          <ThemePicker />
        </div>

        <div className="cuenta-plan cuenta-row">
          <p className="cuenta-plan-name">Sesiones</p>
          {!confirmingLogoutAll ? (
            <button
              type="button"
              className="btn-ghost"
              onClick={() => setConfirmingLogoutAll(true)}
            >
              <LogOut size={16} /> Cerrar todas las sesiones
            </button>
          ) : (
            <div className="cuenta-confirm">
              <span>Se cerrará esta sesión y cualquier otra sesión web activa. ¿Continuar?</span>
              <div className="cuenta-confirm-actions">
                <button
                  type="button"
                  className="btn-ghost"
                  disabled={logoutAllPending}
                  onClick={onLogoutAll}
                >
                  {logoutAllPending ? "Cerrando…" : "Sí, cerrar todas"}
                </button>
                <button
                  type="button"
                  className="access-text-btn"
                  onClick={() => setConfirmingLogoutAll(false)}
                >
                  Volver
                </button>
              </div>
            </div>
          )}
        </div>
      </section>

      {blocked && (
        <section className="card">
          <div className="access-head">
            <span className="access-badge access-badge-warn">
              <ShieldAlert size={14} /> Acceso en pausa
            </span>
            <h2>{(status && BLOCKED_COPY[status]?.title) ?? "Tu acceso está en pausa"}</h2>
            <p className="access-lede">
              {(status && BLOCKED_COPY[status]?.body) ??
                "Necesitas un plan activo para volver a usar la app."}
            </p>
            <p className="access-note-muted">
              Tus análisis y reportes anteriores se conservan según la retención de tu plan y
              vuelven a estar disponibles al reactivar. Solo se bloquean los análisis nuevos.
            </p>
          </div>
          {status !== "past_due" && <PricingPlans
            plans={plans}
            onChoosePlan={onChoosePlan}
            loadingPlanId={checkoutLoadingPlanId}
          />}
          <PaymentNotes />
        </section>
      )}
    </div>
  );
}
