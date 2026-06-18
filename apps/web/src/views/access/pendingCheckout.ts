import type { BillingPlanId } from "../../api/types";

/**
 * Rastrea el checkout en curso entre el redirect a Mercado Pago y el regreso a la app.
 * Permite mostrar el estado "checkout pendiente" y reintentar sin inventar endpoints.
 */
const STORAGE_KEY = "ghi.pending_checkout";

export interface PendingCheckout {
  id: string;
  checkoutUrl: string;
  planId: BillingPlanId;
  planName: string;
  createdAt: number;
}

export function readPendingCheckout(): PendingCheckout | null {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PendingCheckout>;
    if (!parsed.id || !parsed.checkoutUrl) return null;
    return parsed as PendingCheckout;
  } catch {
    return null;
  }
}

export function savePendingCheckout(value: PendingCheckout): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // localStorage no disponible: el flujo sigue funcionando sin estado de espera.
  }
}

export function clearPendingCheckout(): void {
  try {
    window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // no-op
  }
}
