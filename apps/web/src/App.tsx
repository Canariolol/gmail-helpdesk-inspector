import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { API_BASE_URL, api } from "./api/client";
import type {
  AccountStatus,
  AnalysisRun,
  BillingPlan,
  BillingPlanId,
  CheckoutSessionResponse,
  EmailThread,
  EntitlementSnapshot,
  FilterPreset,
  OrgConfig,
  MailboxProviderId,
  ThreadDetail,
  UsageResponse,
} from "./api/types";
import { Sidebar } from "./components/layout/Sidebar";
import type { AppView } from "./components/layout/Sidebar";
import { AccessShell } from "./views/access/AccessShell";
import { deriveAccessState, isBlockedStatus } from "./views/access/accessState";
import { BlockedBanner } from "./views/access/BlockedBanner";
import { CheckoutView } from "./views/access/CheckoutView";
import { MailboxConnectGate } from "./views/access/MailboxConnectGate";
import { LoadingGate } from "./views/access/LoadingGate";
import { PlansModal } from "./views/access/PlansModal";
import { PricingGate } from "./views/access/PricingGate";
import { UsageLimitBanner } from "./views/access/UsageLimitBanner";
import { AyudaView } from "./views/AyudaView";
import { ConfiguracionView } from "./views/ConfiguracionView";
import { CuentaView } from "./views/CuentaView";
import { HilosView } from "./views/HilosView";
import { LandingPage } from "./views/landing/LandingPage";
import { PrivacidadDatosView } from "./views/PrivacidadDatosView";
import { ReportesView } from "./views/ReportesView";
import { ResumenView } from "./views/ResumenView";
import { RunsView } from "./views/RunsView";

// Google conserva su ruta histórica para no romper enlaces vivos; el resto de
// los proveedores usa la ruta neutral.
const CONNECT_URLS: Record<MailboxProviderId, string> = {
  google: `${API_BASE_URL}/gmail/connect/login`,
  microsoft: `${API_BASE_URL}/mailbox/connect/microsoft/login`,
};
const DATA_VIEWS: AppView[] = ["resumen", "hilos", "revision", "anteriores", "reportes"];
const BLOCKED_VIEWS: AppView[] = ["cuenta", "configuracion", "privacidad", "ayuda"];

type ManualReviewPayload = {
  new_classification: EmailThread["classification"];
  is_answered: boolean;
  first_client_message_id: string | null;
  first_internal_reply_message_id: string | null;
  last_internal_message_id: string | null;
  notes: string | null;
};

type PlansModalMode = "checkout" | "change";

export function App() {
  const queryClient = useQueryClient();
  const [view, setView] = useState<AppView>("resumen");
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [threadFilter, setThreadFilter] = useState<string>("all");
  const [plansModalMode, setPlansModalMode] = useState<PlansModalMode | null>(null);
  // Plan elegido para el checkout embebido; muestra el formulario de tarjeta.
  const [checkoutPlan, setCheckoutPlan] = useState<BillingPlan | null>(null);
  // Marca de tiempo del último guardado de revisión exitoso; alimenta la
  // confirmación transitoria del formulario de revisión.
  const [reviewSavedAt, setReviewSavedAt] = useState<number | null>(null);

  const me = useQuery({
    queryKey: ["me"],
    queryFn: () => api<{ email: string }>("/auth/me"),
    retry: false,
  });

  const account = useQuery({
    queryKey: ["account"],
    queryFn: () => api<AccountStatus>("/me/account"),
    enabled: me.isSuccess,
    retry: false,
    staleTime: 30_000,
  });

  const plans = useQuery({
    queryKey: ["public-plans"],
    queryFn: () => api<BillingPlan[]>("/public/plans"),
    enabled: me.isSuccess,
    retry: false,
    staleTime: 300_000,
  });

  const appUnlocked = Boolean(account.data?.entitlement.allowed && account.data?.gmail_connected);
  const isBlocked = Boolean(
    account.data &&
      !account.data.entitlement.allowed &&
      isBlockedStatus(account.data.entitlement.subscription_status),
  );
  const inShell = appUnlocked || isBlocked;

  const usage = useQuery({
    queryKey: ["usage"],
    queryFn: () => api<UsageResponse>("/me/usage"),
    enabled: appUnlocked,
    retry: false,
    staleTime: 60_000,
    refetchInterval: 60_000,
  });

  const mailboxProviders = useQuery({
    queryKey: ["mailbox-providers"],
    queryFn: () => api<{ providers: MailboxProviderId[] }>("/mailbox/providers"),
    retry: false,
    staleTime: Infinity,
  });

  const activeOrgConfig = useQuery({
    queryKey: ["org-config"],
    queryFn: () => api<OrgConfig>("/me/org/config"),
    enabled: inShell,
    retry: false,
    staleTime: 30_000,
  });

  const runs = useQuery({
    queryKey: ["analysis-runs"],
    queryFn: () => api<AnalysisRun[]>("/analysis-runs"),
    enabled: appUnlocked,
    refetchInterval: 3000,
  });

  useEffect(() => {
    if (!selectedRunId && runs.data?.[0]) {
      setSelectedRunId(runs.data[0].id);
    }
  }, [runs.data, selectedRunId]);

  const selectedRun = runs.data?.find((run) => run.id === selectedRunId) ?? null;

  const threads = useQuery({
    queryKey: ["threads", selectedRunId],
    queryFn: () => api<EmailThread[]>(`/analysis-runs/${selectedRunId}/threads`),
    enabled: Boolean(selectedRunId) && appUnlocked,
    refetchInterval: selectedRun?.status === "running" ? 3000 : false,
  });

  const detail = useQuery({
    queryKey: ["thread", selectedRunId, selectedThreadId],
    queryFn: () =>
      api<ThreadDetail>(
        `/analysis-runs/${selectedRunId}/threads/${selectedThreadId}`,
      ),
    enabled: Boolean(selectedRunId && selectedThreadId) && appUnlocked,
  });

  const createRun = useMutation({
    mutationFn: (payload: unknown) =>
      api<AnalysisRun>("/analysis-runs", {
        method: "POST",
        body: JSON.stringify(payload),
      }),
    onSuccess: (run) => {
      setSelectedRunId(run.id);
      setSelectedThreadId(null);
      queryClient.invalidateQueries({ queryKey: ["analysis-runs"] });
      queryClient.invalidateQueries({ queryKey: ["usage"] });
    },
  });

  const startRun = useMutation({
    mutationFn: (runId: string) =>
      api<AnalysisRun>(`/analysis-runs/${runId}/start`, {
        method: "POST",
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["analysis-runs"] }),
  });

  const manualReview = useMutation({
    mutationFn: (payload: ManualReviewPayload) =>
      api<AnalysisRun>(
        `/analysis-runs/${selectedRunId}/threads/${selectedThreadId}/manual-review`,
        {
        method: "PATCH",
        body: JSON.stringify(payload),
        },
      ),
    onSuccess: (updatedRun, payload) => {
      queryClient.setQueryData<AnalysisRun[]>(["analysis-runs"], (current) =>
        current?.map((run) => (run.id === updatedRun.id ? updatedRun : run)),
      );
      queryClient.setQueryData<EmailThread[]>(
        ["threads", selectedRunId],
        (current) =>
          current?.map((thread) =>
            thread.id === selectedThreadId
              ? {
                  ...thread,
                  classification: payload.new_classification,
                  classification_source: "manual",
                  is_valid_client_request:
                    payload.new_classification === "valid_client_request",
                  is_answered: payload.is_answered,
                  first_client_message_id: payload.first_client_message_id,
                  first_internal_reply_message_id:
                    payload.first_internal_reply_message_id,
                  last_internal_message_id: payload.last_internal_message_id,
                  manual_review_required: false,
                  manual_override_applied: true,
                  notes: payload.notes,
                }
              : thread,
          ),
      );
      queryClient.setQueryData<ThreadDetail>(
        ["thread", selectedRunId, selectedThreadId],
        (current) =>
          current
            ? {
                ...current,
                thread: {
                  ...current.thread,
                  classification: payload.new_classification,
                  classification_source: "manual",
                  is_valid_client_request:
                    payload.new_classification === "valid_client_request",
                  is_answered: payload.is_answered,
                  first_client_message_id: payload.first_client_message_id,
                  first_internal_reply_message_id:
                    payload.first_internal_reply_message_id,
                  last_internal_message_id: payload.last_internal_message_id,
                  manual_review_required: false,
                  manual_override_applied: true,
                  notes: payload.notes,
                },
              }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["analysis-runs"] });
      queryClient.invalidateQueries({ queryKey: ["threads", selectedRunId] });
      queryClient.invalidateQueries({
        queryKey: ["thread", selectedRunId, selectedThreadId],
      });
      setReviewSavedAt(Date.now());
    },
  });

  const checkout = useMutation({
    mutationFn: (payload: { planId: BillingPlanId; cardTokenId: string; payerEmail: string }) =>
      api<CheckoutSessionResponse>("/checkout/subscriptions", {
        method: "POST",
        body: JSON.stringify({
          plan_id: payload.planId,
          card_token_id: payload.cardTokenId,
          payer_email: payload.payerEmail,
        }),
      }),
    onSuccess: (response) => {
      const session = response.session;

      // Checkout embebido autorizado: la suscripción ya quedó activa.
      if (session.status === "activated") {
        setCheckoutPlan(null);
        queryClient.invalidateQueries({ queryKey: ["account"] });
        queryClient.invalidateQueries({ queryKey: ["usage"] });
      }
    },
  });

  const changePlan = useMutation({
    mutationFn: (planId: BillingPlanId) =>
      api<EntitlementSnapshot>("/me/subscription/change-plan", {
        method: "POST",
        body: JSON.stringify({ plan_id: planId }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["account"] });
      queryClient.invalidateQueries({ queryKey: ["usage"] });
      setPlansModalMode(null);
    },
  });

  const cancelSubscription = useMutation({
    mutationFn: () => api<EntitlementSnapshot>("/me/subscription/cancel", { method: "POST" }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["account"] }),
  });

  const disconnectGmail = useMutation({
    mutationFn: () => api<void>("/gmail/disconnect", { method: "POST" }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["account"] });
      queryClient.invalidateQueries({ queryKey: ["org-config"] });
    },
  });

  const logoutAllSessions = useMutation({
    mutationFn: () => api<void>("/auth/logout-all", { method: "POST" }),
    onSuccess: () => location.reload(),
  });

  const filterPresets = useQuery({
    queryKey: ["filter-presets"],
    queryFn: () => api<FilterPreset[]>("/me/filter-presets"),
    enabled: inShell,
  });

  const savePreset = useMutation({
    mutationFn: (payload: unknown) =>
      api<FilterPreset>("/me/filter-presets", {
        method: "POST",
        body: JSON.stringify(payload),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["filter-presets"] }),
  });

  const deletePreset = useMutation({
    mutationFn: (id: string) => api<void>(`/me/filter-presets/${id}`, { method: "DELETE" }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["filter-presets"] }),
  });

  const threadItems = threads.data ?? [];
  const reviewCount = useMemo(
    () => threadItems.filter((thread) => thread.manual_review_required).length,
    [threadItems],
  );

  const handleAnalyze = (payload: unknown) => {
    createRun.reset();
    startRun.reset();
    createRun.mutate(payload, {
      onSuccess: (run) => startRun.mutate(run.id),
    });
  };

  const handleSelectRun = (id: string) => {
    setSelectedRunId(id);
    setSelectedThreadId(null);
    setView("resumen");
  };

  const handleLogout = () => {
    api("/auth/logout", { method: "POST" }).then(() => location.reload());
  };

  const handleGoToSetup = () => setView("configuracion");

  const handleChoosePlan = (planId: BillingPlanId) => {
    const plan = plans.data?.find((item) => item.id === planId);
    if (!plan) return;
    checkout.reset();
    setPlansModalMode(null);
    setCheckoutPlan(plan);
  };

  const handlePayCheckout = (data: { cardTokenId: string; payerEmail: string }) => {
    if (!checkoutPlan) return;
    checkout.mutate({
      planId: checkoutPlan.id,
      cardTokenId: data.cardTokenId,
      payerEmail: data.payerEmail,
    });
  };

  const handleBackFromCheckout = () => {
    setCheckoutPlan(null);
    checkout.reset();
  };

  const handleChangePlan = (planId: BillingPlanId) => {
    changePlan.mutate(planId);
  };

  // Al cambiar o cerrar el hilo limpiamos el estado de la mutación para que no
  // arrastre un error/éxito previo al abrir otra trazabilidad.
  const handleSelectThread = (id: string) => {
    setSelectedThreadId(id);
    manualReview.reset();
  };

  const handleCloseThread = () => {
    setSelectedThreadId(null);
    manualReview.reset();
  };

  if (me.isError) {
    return <LandingPage />;
  }

  if (me.isLoading || (me.isSuccess && account.isLoading)) {
    return <LoadingGate />;
  }

  const accountData = account.data;

  if (!accountData) {
    return (
      <AccessShell>
        <div className="access-head access-head-center">
          <h1>No pudimos cargar tu cuenta</h1>
          <p className="access-lede">Revisa tu conexión e inténtalo nuevamente.</p>
        </div>
        <div className="access-actions">
          <button type="button" className="btn-primary" onClick={() => account.refetch()}>
            Reintentar
          </button>
          <button type="button" className="access-text-btn" onClick={handleLogout}>
            Cerrar sesión
          </button>
        </div>
      </AccessShell>
    );
  }

  const accessState = deriveAccessState(accountData);
  const checkoutError = checkout.error?.message ?? null;
  // El estado de carga del pago vive ahora en CheckoutView; las tarjetas de
  // PricingGate solo abren el checkout embebido, sin spinner propio.
  const checkoutLoadingPlanId = null;
  const changePlanLoadingId = changePlan.isPending ? changePlan.variables ?? null : null;
  const planName = accountData.entitlement.plan?.name ?? null;
  const canChangePlan =
    accountData.entitlement.subscription_status === "active" ||
    accountData.entitlement.subscription_status === "trialing";
  const openPlans = () => setPlansModalMode(canChangePlan ? "change" : "checkout");

  if (checkoutPlan) {
    return (
      <CheckoutView
        plan={checkoutPlan}
        onPay={handlePayCheckout}
        onBack={handleBackFromCheckout}
        isSubmitting={checkout.isPending}
        error={checkoutError}
      />
    );
  }

  if (accessState.kind === "pricing") {
    return (
      <PricingGate
        email={accountData.account_email}
        plans={plans.data ?? []}
        onChoosePlan={handleChoosePlan}
        loadingPlanId={checkoutLoadingPlanId}
        error={checkoutError}
        onLogout={handleLogout}
      />
    );
  }

  if (accessState.kind === "connect_gmail") {
    return (
      <MailboxConnectGate
        accountEmail={accountData.account_email}
        providers={mailboxProviders.data?.providers ?? ["google"]}
        connectUrlFor={(provider) => CONNECT_URLS[provider]}
        planName={planName}
        isTrial={accountData.entitlement.subscription_status === "trialing"}
        onLogout={handleLogout}
      />
    );
  }

  // accessState.kind === "blocked" | "ready" → shell de la app (restringido si está bloqueado).
  const currentOrgConfig = activeOrgConfig.data ?? null;
  const runsLimit = usage.data?.limits?.runs_per_month ?? null;
  const runsCreated = usage.data?.usage.runs_created ?? 0;
  const runsLimitReached = !isBlocked && runsLimit !== null && runsCreated >= runsLimit;
  // El cupo mensual de Free se mide en hilos ANALIZADOS, no solo en #análisis.
  // Cupos que BLOQUEAN un análisis nuevo: runs y correos recuperados. El cupo de
  // IA no bloquea (degrada a heurística) y se informa en el reporte del run.
  const retrievedLimit = usage.data?.limits?.retrieved_threads_per_month ?? null;
  const retrievedUsed = usage.data?.usage.retrieved_threads ?? 0;
  const retrievedLimitReached =
    !isBlocked && retrievedLimit !== null && retrievedUsed >= retrievedLimit;
  const effectiveView = isBlocked && DATA_VIEWS.includes(view) ? "cuenta" : view;

  return (
    <div className="app-shell">
      <Sidebar
        view={effectiveView}
        onNavigate={setView}
        email={me.data?.email ?? accountData.account_email}
        reviewCount={reviewCount}
        onLogout={handleLogout}
        availableViews={isBlocked ? BLOCKED_VIEWS : undefined}
      />
      <main className="main-area">
        {isBlocked && <BlockedBanner onReactivate={() => setView("cuenta")} />}
        {retrievedLimitReached && retrievedLimit !== null ? (
          <UsageLimitBanner
            used={retrievedUsed}
            limit={retrievedLimit}
            unitLabel="correos revisados"
            planName={planName}
            onUpgrade={openPlans}
          />
        ) : (
          runsLimitReached &&
          runsLimit !== null && (
            <UsageLimitBanner
              used={runsCreated}
              limit={runsLimit}
              unitLabel="análisis"
              planName={planName}
              onUpgrade={openPlans}
            />
          )
        )}
        {effectiveView === "resumen" && (
          <ResumenView
            run={selectedRun}
            threads={threadItems}
            threadFilter={threadFilter}
            onThreadFilter={setThreadFilter}
            selectedThreadId={selectedThreadId}
            onSelectThread={handleSelectThread}
            onCloseThread={handleCloseThread}
            detail={detail.data}
            onReview={(payload) => manualReview.mutate(payload as ManualReviewPayload)}
            savingReview={manualReview.isPending}
            reviewError={manualReview.error?.message ?? null}
            reviewSavedAt={reviewSavedAt}
            onAnalyze={handleAnalyze}
            analyzing={createRun.isPending || startRun.isPending}
            onStartRun={() => selectedRun && startRun.mutate(selectedRun.id)}
            startingRun={startRun.isPending}
            orgConfig={currentOrgConfig}
            onGoToSetup={handleGoToSetup}
            filterPresets={filterPresets.data ?? []}
            onSavePreset={(payload) => savePreset.mutate(payload)}
            onDeletePreset={(id) => deletePreset.mutate(id)}
            analysisError={createRun.error?.message ?? startRun.error?.message ?? null}
            planName={planName}
            onUpgrade={openPlans}
          />
        )}
        {(effectiveView === "hilos" || effectiveView === "revision") && (
          <HilosView
            threads={threadItems}
            filter={threadFilter}
            onFilter={setThreadFilter}
            forcedFilter={effectiveView === "revision" ? "review" : undefined}
            selectedThreadId={selectedThreadId}
            onSelectThread={handleSelectThread}
            onCloseThread={handleCloseThread}
            detail={detail.data}
            onReview={(payload) => manualReview.mutate(payload as ManualReviewPayload)}
            savingReview={manualReview.isPending}
            reviewError={manualReview.error?.message ?? null}
            reviewSavedAt={reviewSavedAt}
            hasRun={Boolean(selectedRun)}
          />
        )}
        {effectiveView === "anteriores" && (
          <RunsView runs={runs.data ?? []} selectedRunId={selectedRunId} onSelect={handleSelectRun} />
        )}
        {effectiveView === "reportes" && <ReportesView runs={runs.data ?? []} />}
        {effectiveView === "cuenta" && (
          <CuentaView
            account={accountData}
            plans={plans.data ?? []}
            onChoosePlan={handleChoosePlan}
            checkoutLoadingPlanId={checkoutLoadingPlanId}
            onOpenChangePlan={openPlans}
            onCancel={() => cancelSubscription.mutate()}
            cancelPending={cancelSubscription.isPending}
            onDisconnectGmail={() => disconnectGmail.mutate()}
            gmailDisconnectPending={disconnectGmail.isPending}
            onLogoutAll={() => logoutAllSessions.mutate()}
            logoutAllPending={logoutAllSessions.isPending}
            error={
              checkout.error?.message ??
              cancelSubscription.error?.message ??
              disconnectGmail.error?.message ??
              logoutAllSessions.error?.message ??
              null
            }
          />
        )}
        {effectiveView === "configuracion" && (
          <ConfiguracionView
            orgConfig={currentOrgConfig}
            isLoading={activeOrgConfig.isLoading}
            isError={activeOrgConfig.isError}
          />
        )}
        {effectiveView === "privacidad" && <PrivacidadDatosView />}
        {effectiveView === "ayuda" && <AyudaView />}
      </main>

      {plansModalMode && (
        <PlansModal
          mode={plansModalMode}
          plans={plans.data ?? []}
          currentPlanId={accountData.entitlement.plan?.id ?? null}
          currentPlanName={planName}
          onChoosePlan={plansModalMode === "change" ? handleChangePlan : handleChoosePlan}
          loadingPlanId={plansModalMode === "change" ? changePlanLoadingId : null}
          error={plansModalMode === "change" ? changePlan.error?.message ?? null : checkoutError}
          onClose={() => {
            setPlansModalMode(null);
            if (plansModalMode === "change") changePlan.reset();
            else checkout.reset();
          }}
          onCancel={
            canChangePlan && !accountData.entitlement.cancel_at_period_end
              ? () => {
                  cancelSubscription.mutate();
                  setPlansModalMode(null);
                }
              : undefined
          }
          cancelPending={cancelSubscription.isPending}
        />
      )}
    </div>
  );
}
