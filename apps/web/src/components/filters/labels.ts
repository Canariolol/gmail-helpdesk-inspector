import type { GmailLabel } from "../../api/types";

// Token que el backend traduce a operador de búsqueda de Gmail: las etiquetas de
// usuario se referencian por nombre; las de sistema (INBOX, CATEGORY_*) por id.
export function labelToken(label: GmailLabel): string {
  return label.label_type === "user" ? label.name : label.id;
}

// La bandeja de entrada va incluida por defecto; el usuario puede quitarla.
export const INBOX_TOKEN = "INBOX";

// Etiquetas de sistema que no aportan al análisis de helpdesk (el backend ya
// fuerza includeSpamTrash=false, así que incluirlas no haría nada).
const HIDDEN_SYSTEM_LABELS = new Set(["DRAFT", "TRASH", "SPAM", "CHAT"]);

// Nombres legibles en español para las etiquetas de sistema de Gmail. Evita
// mostrar jerga técnica ("CATEGORY_PERSONAL") en la interfaz.
const SYSTEM_LABEL_NAMES: Record<string, string> = {
  INBOX: "Recibidos",
  SENT: "Enviados",
  IMPORTANT: "Importantes",
  STARRED: "Destacados",
  UNREAD: "No leídos",
  CATEGORY_PERSONAL: "Principal",
  CATEGORY_SOCIAL: "Social",
  CATEGORY_PROMOTIONS: "Promociones",
  CATEGORY_UPDATES: "Notificaciones",
  CATEGORY_FORUMS: "Foros",
};

export function humanizeLabelName(label: GmailLabel): string {
  if (label.label_type === "user") return label.name;
  return SYSTEM_LABEL_NAMES[label.id.toUpperCase()] ?? label.name;
}

// Nombre legible a partir de un token suelto (p. ej. el resumen del botón o un
// token guardado en un preset cuya etiqueta ya no exista).
export function tokenDisplayName(token: string, labels: GmailLabel[]): string {
  const match = labels.find((label) => labelToken(label) === token);
  if (match) return humanizeLabelName(match);
  return SYSTEM_LABEL_NAMES[token.toUpperCase()] ?? token;
}

export function isInboxLabel(label: GmailLabel): boolean {
  return label.label_type !== "user" && label.id.toUpperCase() === INBOX_TOKEN;
}

export function isAnalyzableLabel(label: GmailLabel): boolean {
  return !HIDDEN_SYSTEM_LABELS.has(label.id.toUpperCase());
}
