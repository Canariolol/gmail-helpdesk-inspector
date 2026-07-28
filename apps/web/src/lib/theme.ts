/**
 * Temas del sistema de diseño Cadencia (ver design/TOKENS.md).
 * El tema se aplica con data-theme en <html>; Arena es el default
 * y se representa sin atributo. Preferencia por dispositivo en localStorage.
 */

export interface Theme {
  id: string;
  label: string;
  mode: "claro" | "oscuro";
  /** Muestras para el selector: [fondo, texto, acento]. */
  colors: [string, string, string];
}

export const THEMES: Theme[] = [
  { id: "arena", label: "Arena", mode: "claro", colors: ["#f7f3ec", "#26211b", "#f2670c"] },
  { id: "niebla", label: "Niebla", mode: "claro", colors: ["#f4f6f5", "#1b211f", "#2f7d6a"] },
  { id: "grafito", label: "Grafito", mode: "oscuro", colors: ["#1a1816", "#f1ebe3", "#f98a3c"] },
  { id: "musgo", label: "Musgo", mode: "oscuro", colors: ["#141a15", "#eaf1ea", "#5db87a"] },
  { id: "vino", label: "Vino", mode: "oscuro", colors: ["#20131a", "#f4e7ea", "#e8505b"] },
];

const STORAGE_KEY = "mira-theme";
const DEFAULT_THEME = "arena";

export function currentTheme(): string {
  let stored: string | null = null;
  try {
    stored = window.localStorage.getItem(STORAGE_KEY);
  } catch {
    // localStorage puede no estar disponible (modo privado, etc.)
  }
  return THEMES.some((theme) => theme.id === stored) ? (stored as string) : DEFAULT_THEME;
}

export function applyTheme(id: string): void {
  const theme = THEMES.some((candidate) => candidate.id === id) ? id : DEFAULT_THEME;
  if (theme === DEFAULT_THEME) {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = theme;
  }
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // sin persistencia; el tema aplica solo para esta sesión
  }
}

export function initTheme(): void {
  applyTheme(currentTheme());
}
