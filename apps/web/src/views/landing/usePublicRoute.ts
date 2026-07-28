import { useSyncExternalStore } from "react";

// Rutas públicas accesibles por hash (#/pricing, #/about, …). El hash evita
// configurar reescrituras en el hosting y mantiene URLs compartibles.
export type PublicRoute = "home" | "pricing" | "about" | "privacy" | "terms" | "security";

const ROUTES: readonly PublicRoute[] = ["home", "pricing", "about", "privacy", "terms", "security"];

export function publicHref(route: PublicRoute): string {
  return route === "home" ? "#/" : `#/${route}`;
}

function parseHash(): PublicRoute {
  const raw = window.location.hash.replace(/^#\/?/, "").split(/[?#/]/)[0];
  return (ROUTES as readonly string[]).includes(raw) ? (raw as PublicRoute) : "home";
}

function subscribe(callback: () => void): () => void {
  window.addEventListener("hashchange", callback);
  return () => window.removeEventListener("hashchange", callback);
}

export function usePublicRoute(): PublicRoute {
  return useSyncExternalStore(subscribe, parseHash, () => "home");
}
