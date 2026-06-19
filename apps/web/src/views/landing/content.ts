import { FileSearch, Plug, ShieldCheck, Sparkles } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export const BRAND = {
  name: "Gmail Helpdesk Inspector",
  short: "Helpdesk Inspector",
  // La cuenta se crea con WorkOS; Gmail se conecta después, aparte.
  betaTag: "Beta privada · sin reemplazar tu correo",
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
    kicker: BRAND.betaTag,
    title: "¿Cuántas respondiste",
    highlight: "de verdad?",
    subtitle: "Convierte tu casilla de Gmail en métricas auditables. Sin reemplazar tu correo.",
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
    body: "Pedimos solo lectura de Gmail. No enviamos, no etiquetamos, no archivamos y no guardamos el cuerpo completo de tus correos.",
  },
  {
    id: "ia",
    Icon: Sparkles,
    title: "Ninfa, tu auditora con IA",
    body: "Ninfa marca los casos dudosos por ti. La activas solo si quieres y la última palabra siempre es tuya.",
  },
  {
    id: "friccion",
    Icon: Plug,
    title: "Cero fricción",
    body: "Conectas Gmail, defines qué cuenta como solicitud y listo. No reemplaza tu correo: lo mide en silencio.",
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
    title: "Crea tu cuenta",
    body: "Cuenta segura con WorkOS. Eliges plan mensual o activas la prueba de 30 días del plan Pro.",
  },
  {
    n: 2,
    title: "Conecta tu Gmail",
    body: "Recién aquí autorizas Gmail, con acceso de solo lectura. No tocamos tu bandeja.",
  },
  {
    n: 3,
    title: "Lee tus métricas",
    body: "Reportes con trazabilidad al hilo real y una cola clara de casos ambiguos.",
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
  headline: "Solo lectura. Punto.",
  items: ["No envía correos", "No borra nada", "No toca tu bandeja"],
};
