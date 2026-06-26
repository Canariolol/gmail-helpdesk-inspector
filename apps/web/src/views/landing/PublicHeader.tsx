import { BRAND, HERO } from "./content";
import { publicHref } from "./usePublicRoute";

type Props = {
  onLogin: () => void;
  onSignup: () => void;
};

export function PublicHeader({ onLogin, onSignup }: Props) {
  return (
    <header className="lp-focus-nav">
      <a className="lp-brand" href={publicHref("home")} aria-label={`${BRAND.short} · inicio`}>
        <img src="/logo-192.png" alt="" />
        {BRAND.short}
      </a>
      <nav className="lp-focus-nav-links" aria-label="Navegación principal">
        <a className="lp-focus-link" href={publicHref("pricing")}>
          Precios
        </a>
        <a className="lp-focus-link" href={publicHref("about")}>
          Nosotros
        </a>
      </nav>
      <div className="lp-focus-nav-actions">
        <button type="button" className="lp-focus-link" onClick={onLogin}>
          {HERO.focus.login}
        </button>
        <button type="button" className="lp-btn-primary lp-focus-nav-cta" onClick={onSignup}>
          {HERO.focus.cta}
        </button>
      </div>
    </header>
  );
}
