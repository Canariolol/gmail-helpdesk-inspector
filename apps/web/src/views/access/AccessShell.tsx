import type { ReactNode } from "react";
import { BRAND } from "../landing/content";

interface AccessShellProps {
  children: ReactNode;
  /** Usa el layout ancho para la grilla de planes. */
  wide?: boolean;
}

export function AccessShell({ children, wide = false }: AccessShellProps) {
  return (
    <div className="access-shell">
      <header className="access-shell-brand">
        <img src="/logo-192.png" alt="" width={28} height={28} />
        <span>{BRAND.short}</span>
      </header>
      <main className={`access-card${wide ? " access-card-wide" : ""}`}>{children}</main>
      <footer className="access-shell-footer">{BRAND.name} · Beta privada</footer>
    </div>
  );
}
