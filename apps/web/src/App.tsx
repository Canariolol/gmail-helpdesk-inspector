import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api/client";
import type { AnalysisRun, EmailThread, OrgConfig, ThreadDetail } from "./api/types";
import { Sidebar } from "./components/layout/Sidebar";
import type { AppView } from "./components/layout/Sidebar";
import { AyudaView } from "./views/AyudaView";
import { ConfiguracionView } from "./views/ConfiguracionView";
import { HilosView } from "./views/HilosView";
import { LoginView } from "./views/LoginView";
import { ReportesView } from "./views/ReportesView";
import { ResumenView } from "./views/ResumenView";
import { RunsView } from "./views/RunsView";

export function App() {
  const queryClient = useQueryClient();
  const [view, setView] = useState<AppView>("resumen");
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [threadFilter, setThreadFilter] = useState<string>("all");

  const me = useQuery({
    queryKey: ["me"],
    queryFn: () => api<{ email: string }>("/auth/me"),
    retry: false,
  });

  const orgConfig = useQuery({
    queryKey: ["org-config"],
    queryFn: () => api<OrgConfig>("/me/org/config"),
    enabled: me.isSuccess,
    retry: false,
    staleTime: 30_000,
  });

  const runs = useQuery({
    queryKey: ["analysis-runs"],
    queryFn: () => api<AnalysisRun[]>("/analysis-runs"),
    enabled: me.isSuccess,
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
    enabled: Boolean(selectedRunId),
    refetchInterval: selectedRun?.status === "running" ? 3000 : false,
  });

  const detail = useQuery({
    queryKey: ["thread", selectedThreadId],
    queryFn: () => api<ThreadDetail>(`/threads/${selectedThreadId}`),
    enabled: Boolean(selectedThreadId),
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
    mutationFn: (payload: unknown) =>
      api<AnalysisRun>(`/threads/${selectedThreadId}/manual-review`, {
        method: "PATCH",
        body: JSON.stringify(payload),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["analysis-runs"] });
      queryClient.invalidateQueries({ queryKey: ["threads", selectedRunId] });
      queryClient.invalidateQueries({ queryKey: ["thread", selectedThreadId] });
    },
  });

  const threadItems = threads.data ?? [];
  const reviewCount = useMemo(
    () => threadItems.filter((thread) => thread.manual_review_required).length,
    [threadItems],
  );

  const handleAnalyze = (payload: unknown) => {
    createRun.mutate(payload, {
      onSuccess: (run) => startRun.mutate(run.id),
    });
  };

  const handleSelectRun = (id: string) => {
    setSelectedRunId(id);
    setSelectedThreadId(null);
    setView("resumen");
  };

  const handleMetricFilter = (filter: string) => {
    setThreadFilter(filter);
    setView("hilos");
  };

  const handleLogout = () => {
    api("/auth/logout", { method: "POST" }).then(() => location.reload());
  };

  const handleGoToSetup = () => setView("configuracion");

  if (me.isError) {
    return <LoginView />;
  }

  const currentOrgConfig = orgConfig.data ?? null;

  return (
    <div className="app-shell">
      <Sidebar
        view={view}
        onNavigate={setView}
        email={me.data?.email ?? "Conectando..."}
        reviewCount={reviewCount}
        onLogout={handleLogout}
      />
      <main className="main-area">
        {view === "resumen" && (
          <ResumenView
            run={selectedRun}
            threads={threadItems}
            threadFilter={threadFilter}
            onThreadFilter={setThreadFilter}
            onMetricFilter={handleMetricFilter}
            selectedThreadId={selectedThreadId}
            onSelectThread={setSelectedThreadId}
            detail={detail.data}
            onReview={(payload) => manualReview.mutate(payload)}
            savingReview={manualReview.isPending}
            onAnalyze={handleAnalyze}
            analyzing={createRun.isPending || startRun.isPending}
            onStartRun={() => selectedRun && startRun.mutate(selectedRun.id)}
            startingRun={startRun.isPending}
            onViewDetails={() => setView("hilos")}
            orgConfig={currentOrgConfig}
            onGoToSetup={handleGoToSetup}
          />
        )}
        {(view === "hilos" || view === "revision") && (
          <HilosView
            threads={threadItems}
            filter={threadFilter}
            onFilter={setThreadFilter}
            forcedFilter={view === "revision" ? "review" : undefined}
            selectedThreadId={selectedThreadId}
            onSelectThread={setSelectedThreadId}
            detail={detail.data}
            onReview={(payload) => manualReview.mutate(payload)}
            savingReview={manualReview.isPending}
            hasRun={Boolean(selectedRun)}
          />
        )}
        {view === "anteriores" && (
          <RunsView runs={runs.data ?? []} selectedRunId={selectedRunId} onSelect={handleSelectRun} />
        )}
        {view === "reportes" && <ReportesView runs={runs.data ?? []} />}
        {view === "configuracion" && (
          <ConfiguracionView
            orgConfig={currentOrgConfig}
            isLoading={orgConfig.isLoading}
            isError={orgConfig.isError}
          />
        )}
        {view === "ayuda" && <AyudaView />}
      </main>
    </div>
  );
}
