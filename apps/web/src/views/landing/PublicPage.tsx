import { useEffect } from "react";
import { PublicFooter } from "./PublicFooter";
import { PublicHeader } from "./PublicHeader";

type Props = {
  onLogin: () => void;
  onSignup: () => void;
  children: React.ReactNode;
};

// Envoltura común para las páginas públicas secundarias (pricing, nosotros,
// legales): mismo fondo cálido, header y footer que la landing.
export function PublicPage({ onLogin, onSignup, children }: Props) {
  useEffect(() => {
    window.scrollTo(0, 0);
  }, []);

  return (
    <div className="lp lp-focus">
      <PublicHeader onLogin={onLogin} onSignup={onSignup} />
      <main className="lp-public">{children}</main>
      <PublicFooter />
    </div>
  );
}
