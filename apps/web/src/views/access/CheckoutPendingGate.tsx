import { ArrowLeft, Loader2, RefreshCw } from "lucide-react";
import { AccessShell } from "./AccessShell";

interface CheckoutPendingGateProps {
  planName: string;
  /** Reabre la URL de checkout de Mercado Pago para reintentar el pago. */
  onRetry: () => void;
  /** Descarta el checkout en curso y vuelve a la selección de planes. */
  onBackToPlans: () => void;
}

export function CheckoutPendingGate({
  planName,
  onRetry,
  onBackToPlans,
}: CheckoutPendingGateProps) {
  return (
    <AccessShell>
      <div className="access-head access-head-center">
        <span className="access-spinner" aria-hidden="true">
          <Loader2 size={26} />
        </span>
        <h1>Estamos confirmando tu pago</h1>
        <p className="access-lede">
          Mercado Pago está procesando tu suscripción al plan <strong>{planName}</strong>. Esto
          puede tardar unos segundos. Esta página se actualiza sola cuando quede activa.
        </p>
      </div>

      <div className="access-actions">
        <button type="button" className="btn-ghost" onClick={onRetry}>
          <RefreshCw size={16} /> Reintentar pago
        </button>
        <button type="button" className="access-text-btn" onClick={onBackToPlans}>
          <ArrowLeft size={15} /> Volver a los planes
        </button>
      </div>

      <p className="access-note-muted access-note-center">
        Si ya pagaste y este mensaje no cambia, vuelve a los planes y reintenta: no se cobra dos
        veces por una suscripción en curso.
      </p>
    </AccessShell>
  );
}
