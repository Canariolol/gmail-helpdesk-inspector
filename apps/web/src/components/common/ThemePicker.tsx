import { useState } from "react";
import { applyTheme, currentTheme, THEMES } from "../../lib/theme";

export function ThemePicker() {
  const [active, setActive] = useState(currentTheme);

  return (
    <div className="theme-picker" role="radiogroup" aria-label="Tema de la interfaz">
      {THEMES.map((theme) => (
        <button
          key={theme.id}
          type="button"
          role="radio"
          aria-checked={active === theme.id}
          className={`theme-option${active === theme.id ? " theme-option-on" : ""}`}
          onClick={() => {
            applyTheme(theme.id);
            setActive(theme.id);
          }}
        >
          <span className="theme-dots" aria-hidden="true">
            {theme.colors.map((color) => (
              <i key={color} style={{ background: color }} />
            ))}
          </span>
          <span className="theme-option-name">
            {theme.label} <small>{theme.mode}</small>
          </span>
        </button>
      ))}
    </div>
  );
}
