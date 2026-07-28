import {
  CheckCircle2,
  Clock,
  Filter,
  HelpCircle,
  Inbox,
  Reply,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

// Mock autónomo: replica las clasificaciones reales sin importar de la app.
export type DemoClassification =
  | "valid_client_request"
  | "internal"
  | "automated"
  | "newsletter"
  | "spam"
  | "ambiguous";

export type DemoTone = "mint" | "blue" | "teal" | "purple" | "amber" | "gray" | "red" | "orange";

export const DEMO_CLASS_LABELS: Record<DemoClassification, string> = {
  valid_client_request: "Solicitud válida",
  internal: "Interno",
  automated: "Automático",
  newsletter: "Boletín",
  spam: "No deseado",
  ambiguous: "Ambiguo",
};

export const DEMO_CLASS_TONES: Record<DemoClassification, DemoTone> = {
  valid_client_request: "mint",
  internal: "blue",
  automated: "gray",
  newsletter: "purple",
  spam: "red",
  ambiguous: "amber",
};

export interface DemoMessage {
  from: string;
  date: string;
  internal: boolean;
  snippet: string;
}

export interface DemoThread {
  id: string;
  subject: string;
  classification: DemoClassification;
  received: string;
  firstReply: string | null;
  lastSent: string | null;
  valid: boolean;
  answered: boolean;
  reasons?: string[];
  messages: DemoMessage[];
}

export type DemoMetricId =
  | "analizados"
  | "validos"
  | "respondidos"
  | "sin_respuesta"
  | "ambiguos"
  | "ignorados";

export interface DemoMetric {
  id: DemoMetricId;
  label: string;
  value: string;
  tone: DemoTone;
  Icon: LucideIcon;
}

export const DEMO_METRICS: DemoMetric[] = [
  { id: "analizados", label: "Analizados", value: "428", tone: "blue", Icon: Inbox },
  { id: "validos", label: "Válidas", value: "173", tone: "mint", Icon: CheckCircle2 },
  { id: "respondidos", label: "Respondidas", value: "159", tone: "teal", Icon: Reply },
  { id: "sin_respuesta", label: "Sin respuesta", value: "14", tone: "orange", Icon: Clock },
  { id: "ambiguos", label: "Ambiguas", value: "12", tone: "amber", Icon: HelpCircle },
  { id: "ignorados", label: "Ignoradas", value: "84", tone: "purple", Icon: Filter },
];

// --- Pool de hilos ---
const reembolso: DemoThread = {
  id: "t-reembolso",
  subject: "Reembolso pedido #4821",
  classification: "valid_client_request",
  received: "15/06 09:12",
  firstReply: "15/06 09:54 · 42 min",
  lastSent: "15/06 10:20 · 1 h 8 min",
  valid: true,
  answered: true,
  messages: [
    {
      from: "camila.rojas@cliente.com",
      date: "15/06 09:12",
      internal: false,
      snippet:
        "Hola, compré el pedido #4821 pero llegó dañado. Quisiera solicitar el reembolso, ¿cómo procedo?",
    },
    {
      from: "ana@acme.cl",
      date: "15/06 09:54",
      internal: true,
      snippet:
        "Hola Camila, lamentamos lo ocurrido. Ya generamos el reembolso, llegará en 3–5 días hábiles. Te confirmo por aquí.",
    },
  ],
};

const acceso: DemoThread = {
  id: "t-acceso",
  subject: "No puedo acceder a mi cuenta",
  classification: "valid_client_request",
  received: "14/06 16:40",
  firstReply: "14/06 17:10 · 30 min",
  lastSent: "14/06 17:35 · 55 min",
  valid: true,
  answered: true,
  messages: [
    {
      from: "jperez@otraempresa.cl",
      date: "14/06 16:40",
      internal: false,
      snippet: "Buenas, me aparece 'credenciales inválidas' y no logro entrar al panel. ¿Me ayudan?",
    },
    {
      from: "soporte@acme.cl",
      date: "14/06 17:10",
      internal: true,
      snippet: "Hola, te enviamos un enlace para restablecer la contraseña. Avísanos si funciona.",
    },
  ],
};

const cotizacion: DemoThread = {
  id: "t-cotizacion",
  subject: "Cotización 50 licencias",
  classification: "valid_client_request",
  received: "13/06 11:05",
  firstReply: "13/06 12:30 · 1 h 25 min",
  lastSent: "13/06 12:30 · 1 h 25 min",
  valid: true,
  answered: true,
  messages: [
    {
      from: "compras@corp.cl",
      date: "13/06 11:05",
      internal: false,
      snippet: "Necesitamos cotizar 50 licencias del plan Pro para el próximo trimestre.",
    },
    {
      from: "ventas@acme.cl",
      date: "13/06 12:30",
      internal: true,
      snippet: "¡Gracias por escribir! Adjunto la propuesta con el descuento por volumen.",
    },
  ],
};

const factura: DemoThread = {
  id: "t-factura",
  subject: "Error al generar factura",
  classification: "valid_client_request",
  received: "16/06 08:30",
  firstReply: null,
  lastSent: null,
  valid: true,
  answered: false,
  messages: [
    {
      from: "contabilidad@pyme.cl",
      date: "16/06 08:30",
      internal: false,
      snippet: "Al emitir la factura del mes me arroja error 500. ¿Pueden revisar? Es urgente para el cierre.",
    },
  ],
};

const cobro: DemoThread = {
  id: "t-cobro",
  subject: "Reclamo: cobro duplicado",
  classification: "valid_client_request",
  received: "16/06 20:10",
  firstReply: null,
  lastSent: null,
  valid: true,
  answered: false,
  messages: [
    {
      from: "francisca@cliente.com",
      date: "16/06 20:10",
      internal: false,
      snippet: "Me cobraron dos veces la suscripción de junio. Necesito que reviertan uno de los cargos.",
    },
  ],
};

const soporte247: DemoThread = {
  id: "t-247",
  subject: "¿Siguen con soporte 24/7?",
  classification: "valid_client_request",
  received: "16/06 19:45",
  firstReply: null,
  lastSent: null,
  valid: true,
  answered: false,
  messages: [
    {
      from: "it@empresa.cl",
      date: "16/06 19:45",
      internal: false,
      snippet: "Antes de renovar, ¿confirman si mantienen el soporte 24/7 en el plan actual?",
    },
  ],
};

const propuesta: DemoThread = {
  id: "t-propuesta",
  subject: "Re: propuesta comercial",
  classification: "ambiguous",
  received: "15/06 13:00",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  reasons: ["Remitente en dominio interno", "Sin pregunta explícita"],
  messages: [
    {
      from: "diego@acme.cl",
      date: "15/06 13:00",
      internal: true,
      snippet: "Reenvío la propuesta que conversamos. Lo vemos en la reunión.",
    },
  ],
};

const consulta: DemoThread = {
  id: "t-consulta",
  subject: "Consulta (¿es para ustedes?)",
  classification: "ambiguous",
  received: "14/06 10:22",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  reasons: ["Posible reenvío automático", "Asunto genérico"],
  messages: [
    {
      from: "no-reply@formularios.cl",
      date: "14/06 10:22",
      internal: false,
      snippet: "Nuevo mensaje de contacto recibido a través del formulario web del sitio.",
    },
  ],
};

const adjunto: DemoThread = {
  id: "t-adjunto",
  subject: "Fwd: documento adjunto",
  classification: "ambiguous",
  received: "13/06 17:48",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  reasons: ["Cuerpo vacío, solo adjunto"],
  messages: [
    {
      from: "maria@cliente.com",
      date: "13/06 17:48",
      internal: false,
      snippet: "(sin texto) — adjunto: comprobante_transferencia.pdf",
    },
  ],
};

const newsletter: DemoThread = {
  id: "t-newsletter",
  subject: "Acme Weekly — novedades de junio",
  classification: "newsletter",
  received: "16/06 06:00",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  messages: [
    {
      from: "news@marketing.cl",
      date: "16/06 06:00",
      internal: false,
      snippet: "Las 5 tendencias del mes y un descuento especial para ti. Ver en el navegador.",
    },
  ],
};

const premio: DemoThread = {
  id: "t-premio",
  subject: "🎉 Ganaste un premio, reclámalo ya",
  classification: "spam",
  received: "15/06 03:14",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  messages: [
    {
      from: "winner@promo-xyz.info",
      date: "15/06 03:14",
      internal: false,
      snippet: "Felicidades, fuiste seleccionado. Haz clic aquí para reclamar tu premio antes de 24 horas.",
    },
  ],
};

const reunion: DemoThread = {
  id: "t-reunion",
  subject: "Reunión equipo soporte — lunes",
  classification: "internal",
  received: "13/06 18:30",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  messages: [
    {
      from: "ana@acme.cl",
      date: "13/06 18:30",
      internal: true,
      snippet: "Recordatorio: revisamos los casos pendientes el lunes a las 10:00. Lleven sus métricas.",
    },
  ],
};

const respaldo: DemoThread = {
  id: "t-respaldo",
  subject: "Notificación automática: respaldo completado",
  classification: "automated",
  received: "16/06 02:00",
  firstReply: null,
  lastSent: null,
  valid: false,
  answered: false,
  messages: [
    {
      from: "no-reply@backups.acme.cl",
      date: "16/06 02:00",
      internal: false,
      snippet: "El respaldo programado finalizó correctamente. No se requiere ninguna acción.",
    },
  ],
};

export const DEMO_THREADS_BY_METRIC: Record<DemoMetricId, DemoThread[]> = {
  analizados: [reembolso, cobro, propuesta, newsletter, premio],
  validos: [reembolso, acceso, cotizacion, factura, cobro],
  respondidos: [reembolso, acceso, cotizacion],
  sin_respuesta: [factura, cobro, soporte247],
  ambiguos: [propuesta, consulta, adjunto],
  ignorados: [newsletter, premio, reunion, respaldo],
};

export const DEFAULT_DEMO_METRIC: DemoMetricId = "validos";
