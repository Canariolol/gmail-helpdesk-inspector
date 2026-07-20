import { BRAND } from "./content";
import { publicHref } from "./usePublicRoute";

const YEAR = new Date().getFullYear();

export function PublicFooter() {
  return (
    <footer className="lp-public-footer">
      <div className="lp-public-footer-top">
        <div className="lp-public-footer-brand">
          <span className="lp-brand">
            <img src="/logo-192.png" alt="" />
            {BRAND.short}
          </span>
          <p>Métricas auditables para tu casilla de soporte. Sin reemplazar tu correo.</p>
        </div>

        <nav className="lp-public-footer-cols" aria-label="Enlaces del pie de página">
          <div className="lp-public-footer-col">
            <h4>Producto</h4>
            <a href={publicHref("pricing")}>Precios</a>
            <a href={publicHref("about")}>Sobre nosotros</a>
          </div>
          <div className="lp-public-footer-col">
            <h4>Legal</h4>
            <a href={publicHref("privacy")}>Privacidad</a>
            <a href={publicHref("terms")}>Términos y condiciones</a>
            <a href={publicHref("security")}>Seguridad</a>
          </div>
        </nav>
      </div>

      <div className="lp-public-footer-bottom">
        <span>
          © {YEAR} {BRAND.name}
        </span>
        <span>Versión inicial · no afiliado a Google LLC ni a Microsoft Corporation.</span>
      </div>
    </footer>
  );
}
