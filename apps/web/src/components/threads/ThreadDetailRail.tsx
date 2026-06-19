import { X } from "lucide-react";
import { useEffect } from "react";
import type { ThreadDetail } from "../../api/types";
import { EmptyState } from "../common/EmptyState";
import { ThreadDetailPanel } from "./ThreadDetailPanel";

type Props = {
  detail: ThreadDetail | undefined;
  open: boolean;
  onClose: () => void;
  onReview: (payload: unknown) => void;
  saving: boolean;
  reviewError: string | null;
  reviewSavedAt: number | null;
};

// En pantallas anchas se comporta como columna lateral fija (ver .detail-rail en
// layout.css). En pantallas chicas pasa a un drawer deslizante a la derecha con
// backdrop, en vez de apilar la traza bajo todo el listado de hilos.
export function ThreadDetailRail({
  detail,
  open,
  onClose,
  onReview,
  saving,
  reviewError,
  reviewSavedAt,
}: Props) {
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  return (
    <>
      <div className="drawer-backdrop" data-open={open} onClick={onClose} aria-hidden="true" />
      <aside
        className="detail-rail"
        data-open={open}
        role="complementary"
        aria-label="Trazabilidad del hilo"
      >
        <button
          type="button"
          className="detail-rail-close"
          onClick={onClose}
          aria-label="Cerrar trazabilidad"
        >
          <X size={18} />
        </button>
        {detail ? (
          <ThreadDetailPanel
            detail={detail}
            onReview={onReview}
            saving={saving}
            reviewError={reviewError}
            reviewSavedAt={reviewSavedAt}
          />
        ) : open ? (
          <div className="card">
            <EmptyState message="Cargando trazabilidad del hilo…" />
          </div>
        ) : (
          <div className="card">
            <EmptyState message="Selecciona un hilo para revisar la trazabilidad." />
          </div>
        )}
      </aside>
    </>
  );
}
