export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "http://localhost:8080";

const statusMessages: Record<number, string> = {
  400: "La solicitud no es válida.",
  401: "Necesitas iniciar sesión nuevamente.",
  403: "No tienes permiso para realizar esta acción.",
  404: "No se encontró el recurso solicitado.",
  409: "La solicitud entra en conflicto con el estado actual.",
  429: "Demasiadas solicitudes de análisis. Intenta nuevamente más tarde o deja que el análisis automático programado continúe sin consumir tu cuota manual.",
  500: "Ocurrió un error interno.",
  502: "El servicio de auditoría IA no respondió correctamente.",
  503: "El servicio no está disponible temporalmente.",
};

export async function api<T>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...(options.headers ?? {}),
    },
    ...options,
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    const error = typeof body.error === "object" && body.error !== null ? body.error : undefined;
    const message = error?.message ?? body.error ?? statusMessages[response.status] ?? "No se pudo completar la solicitud.";
    const requestId = error?.request_id ?? response.headers.get("x-request-id");
    throw new Error(requestId ? `${message} (ID de soporte: ${requestId})` : message);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return response.json() as Promise<T>;
}
