import { ChevronsLeft, ChevronsRight, Inbox, Plus, Search, Tag, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import type { GmailLabel } from "../../api/types";
import { humanizeLabelName, isAnalyzableLabel, isInboxLabel, labelToken, tokenDisplayName } from "./labels";

type Side = "available" | "included";

type Props = {
  open: boolean;
  labels: GmailLabel[];
  includedTokens: string[];
  onChange: (tokens: string[]) => void;
  onClose: () => void;
};

// Duración del enter/exit; debe coincidir con las transiciones de .lp-modal en CSS.
const TRANSITION_MS = 220;

export function LabelPickerModal({ open, labels, includedTokens, onChange, onClose }: Props) {
  // `mounted` mantiene el nodo en el DOM durante la animación de salida;
  // `active` dispara las clases que animan la entrada en el frame siguiente.
  const [mounted, setMounted] = useState(open);
  const [active, setActive] = useState(false);
  const [query, setQuery] = useState("");
  const [draggingToken, setDraggingToken] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<Side | null>(null);

  useEffect(() => {
    if (open) {
      setMounted(true);
      const frame = requestAnimationFrame(() => setActive(true));
      return () => cancelAnimationFrame(frame);
    }
    setActive(false);
    const timer = setTimeout(() => setMounted(false), TRANSITION_MS);
    return () => clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
    };
  }, [open, onClose]);

  const analyzable = useMemo(() => labels.filter(isAnalyzableLabel), [labels]);

  const availableLabels = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return analyzable.filter((label) => {
      if (includedTokens.includes(labelToken(label))) return false;
      if (!needle) return true;
      return humanizeLabelName(label).toLowerCase().includes(needle);
    });
  }, [analyzable, includedTokens, query]);

  if (!mounted) return null;

  function addToken(token: string) {
    if (includedTokens.includes(token)) return;
    onChange([...includedTokens, token]);
  }

  function removeToken(token: string) {
    onChange(includedTokens.filter((item) => item !== token));
  }

  function handleDrop(side: Side, event: React.DragEvent) {
    event.preventDefault();
    const token = event.dataTransfer.getData("text/plain");
    setDropTarget(null);
    setDraggingToken(null);
    if (!token) return;
    if (side === "included") addToken(token);
    else removeToken(token);
  }

  function allowDrop(side: Side, event: React.DragEvent) {
    event.preventDefault();
    if (dropTarget !== side) setDropTarget(side);
  }

  function renderItem(token: string, name: string, side: Side, icon: React.ReactNode) {
    const isIncluded = side === "included";
    return (
      <li
        key={`${side}-${token}`}
        className={`lp-item${isIncluded ? " is-included" : ""}${draggingToken === token ? " is-dragging" : ""}`}
        draggable
        onDragStart={(event) => {
          event.dataTransfer.setData("text/plain", token);
          event.dataTransfer.effectAllowed = "move";
          setDraggingToken(token);
        }}
        onDragEnd={() => {
          setDraggingToken(null);
          setDropTarget(null);
        }}
        onDoubleClick={() => (isIncluded ? removeToken(token) : addToken(token))}
      >
        <span className="lp-item-icon">{icon}</span>
        <span className="lp-item-name">{name}</span>
        <button
          type="button"
          className="lp-item-action"
          aria-label={isIncluded ? `Quitar ${name}` : `Incluir ${name}`}
          title={isIncluded ? "Quitar" : "Incluir"}
          onClick={() => (isIncluded ? removeToken(token) : addToken(token))}
        >
          {isIncluded ? <X size={14} /> : <Plus size={14} />}
        </button>
      </li>
    );
  }

  return createPortal(
    <div
      className={`lp-modal-backdrop${active ? " is-open" : ""}`}
      role="presentation"
      onClick={onClose}
    >
      <section
        className="lp-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="label-picker-title"
        onClick={(event) => event.stopPropagation()}
      >
        <button type="button" className="lp-close" aria-label="Cerrar" onClick={onClose}>
          <X size={18} />
        </button>

        <div className="lp-head">
          <div>
            <h2 id="label-picker-title">Bandejas y etiquetas a analizar</h2>
            <p>
              Doble clic o arrastra para mover una etiqueta entre listas. Si no incluyes ninguna,
              se analiza la bandeja de entrada.
            </p>
          </div>
        </div>

        <div className="lp-grid">
          <div
            className={`lp-col${dropTarget === "available" ? " is-drop-target" : ""}`}
            onDragOver={(event) => allowDrop("available", event)}
            onDragLeave={() => setDropTarget(null)}
            onDrop={(event) => handleDrop("available", event)}
          >
            <div className="lp-col-head">
              <span>Disponibles</span>
              <span className="lp-col-count">{availableLabels.length}</span>
            </div>
            <div className="lp-search">
              <Search size={14} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Buscar etiqueta…"
                aria-label="Buscar etiqueta"
              />
            </div>
            {availableLabels.length === 0 ? (
              <p className="lp-empty">No hay más etiquetas disponibles.</p>
            ) : (
              <ul className="lp-list">
                {availableLabels.map((label) =>
                  renderItem(
                    labelToken(label),
                    humanizeLabelName(label),
                    "available",
                    isInboxLabel(label) ? <Inbox size={15} /> : <Tag size={15} />,
                  ),
                )}
              </ul>
            )}
          </div>

          <div className="lp-rail">
            <button
              type="button"
              className="lp-rail-btn"
              title="Incluir todas"
              aria-label="Incluir todas las etiquetas"
              disabled={availableLabels.length === 0}
              onClick={() => onChange([...includedTokens, ...availableLabels.map(labelToken)])}
            >
              <ChevronsRight size={16} />
            </button>
            <button
              type="button"
              className="lp-rail-btn"
              title="Quitar todas"
              aria-label="Quitar todas las etiquetas incluidas"
              disabled={includedTokens.length === 0}
              onClick={() => onChange([])}
            >
              <ChevronsLeft size={16} />
            </button>
          </div>

          <div
            className={`lp-col${dropTarget === "included" ? " is-drop-target" : ""}`}
            onDragOver={(event) => allowDrop("included", event)}
            onDragLeave={() => setDropTarget(null)}
            onDrop={(event) => handleDrop("included", event)}
          >
            <div className="lp-col-head">
              <span>Incluidas</span>
              <span className="lp-col-count">{includedTokens.length}</span>
            </div>
            {includedTokens.length === 0 ? (
              <p className="lp-empty">
                Arrastra aquí o haz doble clic para incluir. Sin selección se usa Recibidos.
              </p>
            ) : (
              <ul className="lp-list">
                {includedTokens.map((token) => {
                  const match = analyzable.find((label) => labelToken(label) === token);
                  const icon =
                    match && isInboxLabel(match) ? <Inbox size={15} /> : <Tag size={15} />;
                  return renderItem(token, tokenDisplayName(token, labels), "included", icon);
                })}
              </ul>
            )}
          </div>
        </div>

        <div className="lp-foot">
          <button type="button" className="btn-primary" onClick={onClose}>
            Listo
          </button>
        </div>
      </section>
    </div>,
    document.body,
  );
}
