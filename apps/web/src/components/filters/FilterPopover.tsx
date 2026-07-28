import { useEffect, useId, useRef, useState } from "react";

type Props = {
  triggerContent: React.ReactNode;
  triggerLabel: string;
  panelClassName?: string;
  align?: "start" | "end";
  isActive?: boolean;
  children: (close: () => void) => React.ReactNode;
};

// Popover anclado a su disparador: panel posicionado en flujo (no portal) para
// quedar pegado al trigger. Cierra al hacer clic fuera o con Escape.
export function FilterPopover({
  triggerContent,
  triggerLabel,
  panelClassName,
  align = "start",
  isActive = false,
  children,
}: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const panelId = useId();

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const panelClasses = ["fpop-panel", `fpop-${align}`, panelClassName].filter(Boolean).join(" ");

  return (
    <div className={`fpop${open ? " is-open" : ""}`} ref={rootRef}>
      <button
        type="button"
        className={`fpop-trigger${isActive ? " is-active" : ""}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        aria-label={triggerLabel}
        onClick={() => setOpen((value) => !value)}
      >
        {triggerContent}
      </button>
      {open && (
        <div id={panelId} role="dialog" aria-label={triggerLabel} className={panelClasses}>
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}
