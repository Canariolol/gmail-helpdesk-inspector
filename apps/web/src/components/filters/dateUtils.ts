// Utilidades de fechas/calendario para los selectores de filtros.
// Trabaja con cadenas "YYYY-MM-DD" y fechas-calendario (sin instantes UTC) para
// evitar corrimientos de día por zona horaria.

const HELPDESK_TZ = "America/Santiago";

// Fecha de hoy en la zona del helpdesk, formato YYYY-MM-DD.
export function todayInHelpdeskTz(): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: HELPDESK_TZ,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
}

export type Ymd = { year: number; month: number; day: number }; // month: 1-12

export function parseYmd(value: string): Ymd {
  const [year, month, day] = value.split("-").map(Number);
  return { year, month, day };
}

export function toYmd({ year, month, day }: Ymd): string {
  return `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

// Construye un Date a medianoche local desde "YYYY-MM-DD" (solo fecha-calendario).
export function ymdToDate(value: string): Date {
  const { year, month, day } = parseYmd(value);
  return new Date(year, month - 1, day);
}

export function dateToYmd(date: Date): string {
  return toYmd({ year: date.getFullYear(), month: date.getMonth() + 1, day: date.getDate() });
}

// Suma (o resta) días a una cadena YYYY-MM-DD.
export function addDays(value: string, days: number): string {
  const date = ymdToDate(value);
  date.setDate(date.getDate() + days);
  return dateToYmd(date);
}

// Compara dos cadenas YYYY-MM-DD (orden lexicográfico == cronológico).
export function compareYmd(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

export type MonthCell = { ymd: string; inMonth: boolean };

// Matriz de 6 semanas (42 celdas) lunes→domingo para el mes visible, rellenando
// con días del mes anterior/siguiente.
export function buildMonthGrid(year: number, month: number): MonthCell[] {
  const first = new Date(year, month - 1, 1);
  const offset = (first.getDay() + 6) % 7; // lunes primero (getDay 0=domingo)
  const cells: MonthCell[] = [];
  for (let i = 0; i < 42; i += 1) {
    const date = new Date(year, month - 1, 1 - offset + i);
    cells.push({ ymd: dateToYmd(date), inMonth: date.getMonth() === month - 1 });
  }
  return cells;
}

function stripDots(value: string): string {
  return value.replace(/\./g, "");
}

const DAY_FMT = new Intl.DateTimeFormat("es-CL", { day: "numeric", month: "short" });
const DAY_FMT_YEAR = new Intl.DateTimeFormat("es-CL", { day: "numeric", month: "short", year: "numeric" });

// Etiqueta corta "25 jun"; añade el año si difiere del actual.
export function formatDayLabel(value: string): string {
  const date = ymdToDate(value);
  const fmt = date.getFullYear() === new Date().getFullYear() ? DAY_FMT : DAY_FMT_YEAR;
  return stripDots(fmt.format(date));
}

// Título del calendario: "Junio 2026".
export function monthTitle(year: number, month: number): string {
  const name = new Intl.DateTimeFormat("es-CL", { month: "long" }).format(new Date(year, month - 1, 1));
  return `${name.charAt(0).toUpperCase()}${name.slice(1)} ${year}`;
}
