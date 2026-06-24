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
  ThreadDetail,
  UsageResponse,
} from "./api/types";
import { Sidebar } from "./components/layout/Sidebar";
import type { AppView } from "./components/layout/Sidebar";
import { AccessShell } from "./views/access/AccessShell";
import { deriveAccessState, isBlockedStatus } from "./views/access/accessState";
import { BlockedBanner } from "./views/access/BlockedBanner";
import { CheckoutPendingGate } from "./views/access/CheckoutPendingGate";
import { GmailConnectGate } from "./views/access/GmailConnectGate";
import { LoadingGate } from "./views/access/LoadingGate";
import {
  clearPendingCheckout,
  readPendingCheckout,
  savePendingCheckout,
} from "./views/access/pendingCheckout";
import type { PendingCheckout } from "./views/access/pendingCheckout";
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

const GMAIL_CONNECT_URL = `${API_BASE_URL}/gmail/connect/login`;
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

export function App() {
  const queryClient = useQueryClient();
  const [view, setView] = useState<AppView>("resumen");
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [threadFilter, setThreadFilter] = useState<string>("all");
  const [pendingCheckout, setPendingCheckout] = useState<PendingCheckout | null>(() =>
    readPendingCheckout(),
  );
  const [showPlansModal, setShowPlansModal] = useState(false);
  // Marca de tiempo del último guardado de revisión exitoso; alimenta la
  // confirmación transitoria del formulario de revisión.
  const [reviewSavedAt, setReviewSavedAt] = useState<number | null>(null);

  const hasPendingCheckout = pendingCheckout !== null;

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
    // Mientras hay un checkout en curso, espera a que el webhook de Mercado Pago active el plan.
    refetchInterval: (query) => {
      const data = query.state.data;
      if (!data || !hasPendingCheckout) return false;
      return data.entitlement.allowed ? false : 5000;
    },
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

  // Limpia el checkout en curso una vez que el plan queda activo o entra en bloqueo.
  useEffect(() => {
    if (!account.data || !pendingCheckout) return;
    const entitlement = account.data.entitlement;
    if (entitlement.allowed || isBlockedStatus(entitlement.subscription_status)) {
      clearPendingCheckout();
      setPendingCheckout(null);
    }
  }, [account.data, pendingCheckout]);

  // Retorno de Mercado Pago: ?session_id=... → reconstruir el estado de "checkout pendiente".
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const sessionId = params.get("session_id");
    if (!sessionId) return;
    let cancelled = false;
    api<CheckoutSessionResponse>(`/checkout/sessions/${sessionId}`)
      .then((response) => {
        if (cancelled) return;
        const session = response.session;
        const pending: PendingCheckout = {
          id: session.id,
          checkoutUrl: session.checkout_url ?? "",
          planId: session.plan_id,
          planName: session.plan_id,
          createdAt: Date.now(),
        };
        savePendingCheckout(pending);
        setPendingCheckout(pending);
      })
      .catch(() => {
        /* sesión de checkout no encontrada: el polling de la cuenta resolverá igual */
      })
      .finally(() => {
        window.history.replaceState({}, document.title, "/");
      });
    return () => {
      cancelled = true;
    };
  }, []);

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
    mutationFn: (planId: BillingPlanId) =>
      api<CheckoutSessionResponse>("/checkout/subscriptions", {
        method: "POST",
        body: JSON.stringify({ plan_id: planId }),
      }),
    onSuccess: (response) => {
      const session = response.session;
      if (!session.checkout_url) return;
      const plan = plans.data?.find((item) => item.id === session.plan_id);
      const pending: PendingCheckout = {
        id: session.id,
        checkoutUrl: session.checkout_url,
        planId: session.plan_id,
        planName: plan?.name ?? session.plan_id,
        createdAt: Date.now(),
      };
      savePendingCheckout(pending);
      setPendingCheckout(pending);
      window.location.href = session.checkout_url;
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
      setShowPlansModal(false);
    },
  });

  const cancelSubscription = useMutation({
    mutationFn: () => api<EntitlementSnapshot>("/me/subscription/cancel", { method: "POST" }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["account"] }),
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
    checkout.mutate(planId);
  };

  const handleChangePlan = (planId: BillingPlanId) => {
    changePlan.mutate(planId);
  };

  const handleRetryCheckout = () => {
    if (pendingCheckout?.checkoutUrl) {
      window.location.href = pendingCheckout.checkoutUrl;
    }
  };

  const handleBackToPlans = () => {
    clearPendingCheckout();
    setPendingCheckout(null);
    checkout.reset();
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

  const accessState = deriveAccessState(accountData, hasPendingCheckout);
  const checkoutError = checkout.error?.message ?? null;
  const checkoutLoadingPlanId = checkout.isPending ? checkout.variables ?? null : null;
  const changePlanLoadingId = changePlan.isPending ? changePlan.variables ?? null : null;
  const planName = accountData.entitlement.plan?.name ?? null;

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

  if (accessState.kind === "checkout_pending") {
    return (
      <CheckoutPendingGate
        planName={pendingCheckout?.planName ?? planName ?? "tu plan"}
        onRetry={handleRetryCheckout}
        onBackToPlans={handleBackToPlans}
      />
    );
  }

  if (accessState.kind === "connect_gmail") {
    return (
      <GmailConnectGate
        accountEmail={accountData.account_email}
        connectUrl={GMAIL_CONNECT_URL}
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
        {runsLimitReached && runsLimit !== null && (
          <UsageLimitBanner
            runsCreated={runsCreated}
            runsLimit={runsLimit}
            planName={planName}
            onUpgrade={() => setShowPlansModal(true)}
          />
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
            onOpenChangePlan={() => setShowPlansModal(true)}
            onCancel={() => cancelSubscription.mutate()}
            cancelPending={cancelSubscription.isPending}
            error={checkout.error?.message ?? cancelSubscription.error?.message ?? null}
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

      {showPlansModal && (
        <PlansModal
          mode="change"
          plans={plans.data ?? []}
          currentPlanId={accountData.entitlement.plan?.id ?? null}
          currentPlanName={planName}
          onChoosePlan={handleChangePlan}
          loadingPlanId={changePlanLoadingId}
          error={changePlan.error?.message ?? null}
          onClose={() => {
            setShowPlansModal(false);
            changePlan.reset();
          }}
        />
      )}
    </div>
  );
}
