import { useState } from "react";
import { applyTheme, currentTheme, THEMES } from "../../lib/theme";

/** Selector de tema flotante y discreto para la landing (útil para probar paletas). */
export function ThemeDots() {
  const [active, setActive] = useState(currentTheme);

  return (
    <div className="lp-theme-dots" role="radiogroup" aria-label="Tema de la interfaz">
      {THEMES.map((theme) => (
        <button
          key={theme.id}
          type="button"
          role="radio"
          aria-checked={active === theme.id}
          title={`${theme.label} · ${theme.mode}`}
          className={`lp-theme-dot${active === theme.id ? " on" : ""}`}
          style={{ background: theme.colors[2] }}
          onClick={() => {
            applyTheme(theme.id);
            setActive(theme.id);
          }}
        >
          <span className="sr-only">{theme.label}</span>
        </button>
      ))}
    </div>
  );
}
