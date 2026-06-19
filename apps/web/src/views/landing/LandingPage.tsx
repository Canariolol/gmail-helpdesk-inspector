import { API_BASE_URL } from "../../api/client";
import { FocusLanding } from "./variants/FocusLanding";

// AuthKit maneja ingreso y registro en el mismo flujo; `screen_hint` decide en qué
// pantalla aterriza el usuario según el botón que pulsó.
const authUrl = (hint: "sign-up" | "sign-in") =>
  `${API_BASE_URL}/auth/workos/login?screen_hint=${hint}`;

export function LandingPage() {
  return (
    <FocusLanding
      onSignup={() => {
        window.location.href = authUrl("sign-up");
      }}
      onLogin={() => {
        window.location.href = authUrl("sign-in");
      }}
    />
  );
}
