import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BarChart3, Bot, CheckCircle2, Clock, Filter, LogIn, Mail, RefreshCw, Send, ShieldCheck, Timer } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { API_BASE_URL, api } from "./api/client";
import type { AnalysisRun, Classification, EmailThread, ThreadDetail } from "./api/types";

const classificationLabels: Record<Classification, string> = {
  valid_client_request: "Solicitud válida",
  internal: "Interno",
  automated: "Automático",
  newsletter: "Newsletter",
  spam: "Spam",
  misc: "Ignorado",
  ambiguous: "Ambiguo",
};

export function App() {
  const queryClient = useQueryClient();
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [threadFilter, setThreadFilter] = useState<string>("all");

  const me = useQuery({
    queryKey: ["me"],
    queryFn: () => api<{ email: string }>("/auth/me"),
    retry: false,
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

  const filteredThreads = useMemo(() => {
    const items = threads.data ?? [];
    if (threadFilter === "all") return items;
    if (threadFilter === "answered") return items.filter((thread) => thread.is_answered);
    if (threadFilter === "unanswered") return items.filter((thread) => thread.is_valid_client_request && !thread.is_answered);
    if (threadFilter === "review") return items.filter((thread) => thread.manual_review_required);
    return items.filter((thread) => thread.classification === threadFilter);
  }, [threads.data, threadFilter]);

  if (me.isError) {
    return <LoginView />;
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <img src="/logo.png" alt="" />
          <div>
            <strong>Gmail Inspector</strong>
            <span>{me.data?.email ?? "Conectando..."}</span>
          </div>
        </div>
        <button className="icon-button" onClick={() => api("/auth/logout", { method: "POST" }).then(() => location.reload())}>
          <LogIn size={18} />
          Salir
        </button>
        <section className="run-list">
          <h2>Análisis</h2>
          {(runs.data ?? []).map((run) => (
            <button
              key={run.id}
              className={run.id === selectedRunId ? "run-item active" : "run-item"}
              onClick={() => setSelectedRunId(run.id)}
            >
              <span>{run.config.date_from} {run.config.time_from ?? "00:00"} a {run.config.date_to} {run.config.time_to ?? "23:59"}</span>
              <small>{run.status}</small>
            </button>
          ))}
        </section>
      </aside>

      <section className="workspace">
        <SetupPanel
          loading={createRun.isPending}
          onSubmit={(payload) => createRun.mutate(payload)}
        />

        {selectedRun && (
          <>
            <ProgressPanel run={selectedRun} onStart={() => startRun.mutate(selectedRun.id)} starting={startRun.isPending} />
            <Dashboard run={selectedRun} onFilter={setThreadFilter} />
            <ThreadTable
              threads={filteredThreads}
              filter={threadFilter}
              onFilter={setThreadFilter}
              selectedThreadId={selectedThreadId}
              onSelect={setSelectedThreadId}
            />
          </>
        )}
      </section>

      <aside className="detail-pane">
        {detail.data ? (
          <ThreadDetailPanel
            detail={detail.data}
            onReview={(payload) => manualReview.mutate(payload)}
            saving={manualReview.isPending}
          />
        ) : (
          <div className="empty-detail">
            <Mail size={30} />
            <span>Selecciona un hilo para revisar la trazabilidad.</span>
          </div>
        )}
      </aside>
    </main>
  );
}

function LoginView() {
  return (
    <main className="login-screen">
      <img src="/logo.png" alt="" />
      <h1>Gmail Helpdesk Inspector</h1>
      <p>Audita una casilla Gmail de soporte con métricas trazables y verificación IA.</p>
      <a className="primary-action" href={`${API_BASE_URL}/auth/google/login`}>
        <LogIn size={18} />
        Conectate, preciosura =*
      </a>
    </main>
  );
}

function SetupPanel({ loading, onSubmit }: { loading: boolean; onSubmit: (payload: unknown) => void }) {
  const [dateFrom, setDateFrom] = useState("2026-06-01");
  const [dateTo, setDateTo] = useState("2026-06-12");
  const [timeFrom, setTimeFrom] = useState("00:00");
  const [timeTo, setTimeTo] = useState("23:59");
  const [domains, setDomains] = useState("empresa.cl");
  const [ignoredDomains, setIgnoredDomains] = useState("google.com,calendar.google.com");
  const [ignoredKeywords, setIgnoredKeywords] = useState("newsletter,boletín,promoción");

  return (
    <form
      className="setup-panel"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({
          date_from: dateFrom,
          date_to: dateTo,
          time_from: timeFrom,
          time_to: timeTo,
          timezone: "America/Santiago",
          internal_domains: split(domains),
          ignored_senders: [],
          ignored_domains: split(ignoredDomains),
          ignored_keywords: split(ignoredKeywords),
        });
      }}
    >
      <label>
        Desde
        <input type="date" value={dateFrom} onChange={(event) => setDateFrom(event.target.value)} />
      </label>
      <label>
        Hasta
        <input type="date" value={dateTo} onChange={(event) => setDateTo(event.target.value)} />
      </label>
      <label>
        Hora desde
        <input type="time" value={timeFrom} onChange={(event) => setTimeFrom(event.target.value)} />
      </label>
      <label>
        Hora hasta
        <input type="time" value={timeTo} onChange={(event) => setTimeTo(event.target.value)} />
      </label>
      <label>
        Dominios internos
        <input value={domains} onChange={(event) => setDomains(event.target.value)} />
      </label>
      <label>
        Dominios ignorados
        <input value={ignoredDomains} onChange={(event) => setIgnoredDomains(event.target.value)} />
      </label>
      <label>
        Palabras ignoradas
        <input value={ignoredKeywords} onChange={(event) => setIgnoredKeywords(event.target.value)} />
      </label>
      <button className="primary-action" disabled={loading}>
        <RefreshCw size={18} />
        Crear análisis
      </button>
    </form>
  );
}

function ProgressPanel({ run, onStart, starting }: { run: AnalysisRun; onStart: () => void; starting: boolean }) {
  const progress = run.total_candidate_threads ? Math.round((run.processed_threads / run.total_candidate_threads) * 100) : 0;
  return (
    <section className="progress-panel">
      <div>
        <strong>{run.progress_message}</strong>
        <span>{run.processed_threads}/{run.total_candidate_threads} hilos · Entrada {run.metrics.ai_input_tokens} · Salida {run.metrics.ai_output_tokens} tokens</span>
      </div>
      <div className="progress-bar"><span style={{ width: `${progress}%` }} /></div>
      <button className="icon-button" disabled={run.status === "running" || starting} onClick={onStart}>
        <Bot size={18} />
        Analizar
      </button>
    </section>
  );
}

function Dashboard({ run, onFilter }: { run: AnalysisRun; onFilter: (filter: string) => void }) {
  const metrics = run.metrics;
  const cards = [
    ["Total", metrics.total_threads, "all", BarChart3],
    ["Válidas", metrics.valid_requests, "valid_client_request", CheckCircle2],
    ["Respondidas", metrics.answered, "answered", ShieldCheck],
    ["Sin respuesta", metrics.unanswered, "unanswered", Clock],
    ["Ambiguas", metrics.ambiguous, "review", Filter],
    ["T. medio respuesta", formatDuration(metrics.avg_first_response_minutes), "answered", Timer],
    ["P90 respuesta", formatDuration(metrics.p90_first_response_minutes), "answered", Clock],
    ["Cierre medio", formatDuration(metrics.avg_resolution_minutes), "answered", Send],
  ] as const;
  const chartData = [
    { name: "Válidas", value: metrics.valid_requests },
    { name: "Respondidas", value: metrics.answered },
    { name: "Sin respuesta", value: metrics.unanswered },
    { name: "Ignoradas", value: metrics.ignored },
    { name: "Ambiguas", value: metrics.ambiguous },
  ];
  return (
    <section className="dashboard">
      <div className="metric-grid">
        {cards.map(([label, value, filter, Icon]) => (
          <button className="metric-card" key={label} onClick={() => onFilter(filter)}>
            <Icon size={20} />
            <span>{label}</span>
            <strong>{value}</strong>
          </button>
        ))}
        <div className="metric-card passive">
          <Bot size={20} />
          <span>Confianza</span>
          <strong>{Math.round(metrics.report_confidence * 100)}%</strong>
        </div>
      </div>
      <div className="chart-panel">
        <ResponsiveContainer width="100%" height={180}>
          <BarChart data={chartData}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} />
            <XAxis dataKey="name" />
            <YAxis allowDecimals={false} />
            <Tooltip />
            <Bar dataKey="value" fill="#2563eb" radius={[4, 4, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </section>
  );
}

function ThreadTable(props: {
  threads: EmailThread[];
  filter: string;
  onFilter: (filter: string) => void;
  selectedThreadId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <section className="thread-section">
      <div className="table-toolbar">
        <h2>Hilos auditables</h2>
        <select value={props.filter} onChange={(event) => props.onFilter(event.target.value)}>
          <option value="all">Todos</option>
          <option value="valid_client_request">Solicitudes válidas</option>
          <option value="answered">Respondidas</option>
          <option value="unanswered">Sin respuesta</option>
          <option value="review">Revisión pendiente</option>
          <option value="misc">Ignorados</option>
        </select>
      </div>
      <div className="thread-table">
        <div className="thread-row header">
          <span>Asunto</span>
          <span>Recepción</span>
          <span>1a respuesta</span>
          <span>Último envío</span>
          <span>Estado</span>
          <span>Revisión</span>
        </div>
        {props.threads.map((thread) => (
          <button
            key={thread.id}
            className={thread.id === props.selectedThreadId ? "thread-row active" : "thread-row"}
            onClick={() => props.onSelect(thread.id)}
          >
            <span className="subject">{thread.subject}</span>
            <span>{formatDateTime(thread.first_client_message_at)}</span>
            <span>{formatDateTime(thread.first_internal_reply_at)}</span>
            <span>{formatDateTime(thread.last_internal_message_at)}</span>
            <span>
              <span className={`badge ${thread.classification}`}>{classificationLabels[thread.classification]}</span>
            </span>
            <span>{thread.manual_review_required ? "Revisar" : thread.classification_source}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

function ThreadDetailPanel({ detail, onReview, saving }: { detail: ThreadDetail; onReview: (payload: unknown) => void; saving: boolean }) {
  const [classification, setClassification] = useState<Classification>(detail.thread.classification);
  const [answered, setAnswered] = useState(detail.thread.is_answered);
  const [valid, setValid] = useState(detail.thread.is_valid_client_request);
  const [firstClient, setFirstClient] = useState(detail.thread.first_client_message_id ?? "");
  const [firstReply, setFirstReply] = useState(detail.thread.first_internal_reply_message_id ?? "");
  const [lastInternal, setLastInternal] = useState(detail.thread.last_internal_message_id ?? "");
  const [notes, setNotes] = useState("");

  useEffect(() => {
    setClassification(detail.thread.classification);
    setAnswered(detail.thread.is_answered);
    setValid(detail.thread.is_valid_client_request);
    setFirstClient(detail.thread.first_client_message_id ?? "");
    setFirstReply(detail.thread.first_internal_reply_message_id ?? "");
    setLastInternal(detail.thread.last_internal_message_id ?? "");
  }, [detail.thread.id]);

  return (
    <div className="thread-detail">
      <h2>{detail.thread.subject}</h2>
      <div className="trace-summary">
        <span><strong>Recepción</strong>{formatDateTime(detail.thread.first_client_message_at)}</span>
        <span><strong>1a respuesta</strong>{formatDateTime(detail.thread.first_internal_reply_at)} · {formatDuration(detail.thread.response_time_minutes)}</span>
        <span><strong>Último envío</strong>{formatDateTime(detail.thread.last_internal_message_at)} · {formatDuration(detail.thread.resolution_time_minutes)}</span>
      </div>
      <div className="reason-box">
        {detail.thread.reasons.map((reason) => <span key={reason}>{reason}</span>)}
      </div>
      <div className="timeline">
        {detail.messages.map((message) => (
          <article key={message.id} className={message.is_internal ? "message internal" : "message external"}>
            <header>
              <strong>{message.from_email}</strong>
              <time>{new Date(message.date).toLocaleString()}</time>
            </header>
            <p>{message.snippet || "Sin snippet disponible"}</p>
          </article>
        ))}
      </div>
      <form
        className="review-form"
        onSubmit={(event) => {
          event.preventDefault();
          onReview({
            reviewer_label: "local-user",
            new_classification: classification,
            is_valid_client_request: valid,
            is_answered: answered,
            first_client_message_id: firstClient || null,
            first_internal_reply_message_id: firstReply || null,
            last_internal_message_id: lastInternal || null,
            notes: notes || null,
          });
        }}
      >
        <label>
          Clasificación
          <select value={classification} onChange={(event) => setClassification(event.target.value as Classification)}>
            {Object.entries(classificationLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select>
        </label>
        <label className="checkline">
          <input type="checkbox" checked={valid} onChange={(event) => setValid(event.target.checked)} />
          Solicitud válida
        </label>
        <label className="checkline">
          <input type="checkbox" checked={answered} onChange={(event) => setAnswered(event.target.checked)} />
          Respondido
        </label>
        <label>
          Primer mensaje cliente
          <select value={firstClient} onChange={(event) => setFirstClient(event.target.value)}>
            <option value="">Sin seleccionar</option>
            {detail.messages
              .filter((message) => message.is_external && !message.is_automated)
              .map((message) => <option key={message.id} value={message.id}>{message.from_email} · {formatDateTime(message.date)}</option>)}
          </select>
        </label>
        <label>
          Primera respuesta interna
          <select value={firstReply} onChange={(event) => setFirstReply(event.target.value)}>
            <option value="">Sin seleccionar</option>
            {detail.messages
              .filter((message) => message.is_internal && !message.is_automated)
              .map((message) => <option key={message.id} value={message.id}>{message.from_email} · {formatDateTime(message.date)}</option>)}
          </select>
        </label>
        <label>
          Último envío interno
          <select value={lastInternal} onChange={(event) => setLastInternal(event.target.value)}>
            <option value="">Sin seleccionar</option>
            {detail.messages
              .filter((message) => message.is_internal && !message.is_automated)
              .map((message) => <option key={message.id} value={message.id}>{message.from_email} · {formatDateTime(message.date)}</option>)}
          </select>
        </label>
        <label>
          Nota
          <textarea value={notes} onChange={(event) => setNotes(event.target.value)} />
        </label>
        <button className="primary-action" disabled={saving}>Guardar revisión</button>
      </form>
    </div>
  );
}

function split(value: string) {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

function formatDateTime(value: string | null | undefined) {
  if (!value) return "Sin dato";
  return new Intl.DateTimeFormat("es-CL", {
    timeZone: "America/Santiago",
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function formatDuration(value: number | null | undefined) {
  if (value === null || value === undefined) return "Sin dato";
  const minutes = Math.max(0, Math.round(value));
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours < 24) return rest ? `${hours} h ${rest} min` : `${hours} h`;
  const days = Math.floor(hours / 24);
  const dayHours = hours % 24;
  return dayHours ? `${days} d ${dayHours} h` : `${days} d`;
}
