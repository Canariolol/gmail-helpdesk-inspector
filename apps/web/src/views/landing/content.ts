import { FileSearch, Plug, ShieldCheck, Sparkles } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export const BRAND = {
  name: "Mira Helpdesk",
  short: "Mira",
  // La cuenta se crea con WorkOS; la casilla se conecta después, aparte.
  publicTag: "Versión inicial · sin reemplazar tu correo",
};

export interface HeroCopy {
  kicker: string;
  title: string;
  highlight: string;
  subtitle: string;
  cta: string;
  login: string;
}

export const HERO: { focus: HeroCopy } = {
  focus: {
    kicker: BRAND.publicTag,
    title: "¿Cuántas respondiste",
    highlight: "de verdad?",
    subtitle: "Convierte tu casilla de soporte en métricas auditables. Sin reemplazar tu correo.",
    cta: "Crear cuenta",
    login: "Ingresar",
  },
};

export interface Advantage {
  id: string;
  Icon: LucideIcon;
  title: string;
  body: string;
}

export const ADVANTAGES: Advantage[] = [
  {
    id: "trazabilidad",
    Icon: FileSearch,
    title: "Métricas que puedes auditar",
    body: "Cada número lleva al hilo y al mensaje que lo originó. Nada de cajas negras: si no cuadra, lo abres y lo revisas.",
  },
  {
    id: "privacidad",
    Icon: ShieldCheck,
    title: "Privacidad por diseño",
    body: "Leemos tu casilla para el análisis. No enviamos desde ella, no etiquetamos, no archivamos y no guardamos el cuerpo completo de tus correos.",
  },
  {
    id: "ia",
    Icon: Sparkles,
    title: "Mira, tu auditora con IA",
    body: "La auditoría con IA requiere tu autorización y puedes desactivarla. Sus propuestas pueden contener errores: revisa los casos dudosos y confirma la clasificación.",
  },
  {
    id: "friccion",
    Icon: Plug,
    title: "Conserva tu correo",
    body: "Conectas tu casilla y defines qué cuenta como solicitud. Mira mide los hilos sin cambiar tu forma de atenderlos.",
  },
];

export interface Step {
  n: number;
  title: string;
  body: string;
}

export const STEPS: Step[] = [
  {
    n: 1,
    title: "Conecta tu correo",
    body: "Enlazas una casilla de Gmail, Microsoft o un proveedor con IMAP sobre TLS. Tu administrador puede tener que autorizar el acceso.",
  },
  {
    n: 2,
    title: "Ajústalo a tu manera",
    body: "Defines quién solicita y quién responde, qué remitentes ignorar y el horario del análisis. La IA se habilita con tu autorización.",
  },
  {
    n: 3,
    title: "Métricas automáticas",
    body: "Ejecutas un análisis o programas informes por correo. Ambos usan los cupos de tu plan; las métricas reflejan las respuestas observadas en la casilla.",
  },
];

// --- Reportes automáticos (sección destacada) ---
export const REPORTS = {
  eyebrow: "Reportes automáticos",
  title: "Tus métricas llegan solas a tu correo.",
  body: "Programa un reporte y recíbelo sin entrar a la app. Tú eliges los días, la hora y a quién le llega.",
  bullets: [
    "Días hábiles a las 08:00, o cuando prefieras",
    "A los destinatarios que tú definas",
    "Solo métricas: sin exponer asuntos de clientes",
  ],
};

// --- Banda de confianza (franja full-width) ---
export const TRUST_BAND = {
  headline: "Tu correo sin cambios.",
  items: ["No envía desde tu casilla", "No borra nada", "No modifica tu bandeja"],
};
