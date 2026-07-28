import { API_BASE_URL } from "../../api/client";
import { AboutPage } from "./pages/AboutPage";
import { LegalPage } from "./pages/LegalPage";
import { PRIVACY, SECURITY, TERMS } from "./pages/legalContent";
import { PricingPage } from "./pages/PricingPage";
import { usePublicRoute } from "./usePublicRoute";
import { FocusLanding } from "./variants/FocusLanding";

// AuthKit maneja ingreso y registro en el mismo flujo; `screen_hint` decide en qué
// pantalla aterriza el usuario según el botón que pulsó.
const authUrl = (hint: "sign-up" | "sign-in") =>
  `${API_BASE_URL}/auth/workos/login?screen_hint=${hint}`;

export function LandingPage() {
  const route = usePublicRoute();
  const onSignup = () => {
    window.location.href = authUrl("sign-up");
  };
  const onLogin = () => {
    window.location.href = authUrl("sign-in");
  };

  switch (route) {
    case "pricing":
      return <PricingPage onLogin={onLogin} onSignup={onSignup} />;
    case "about":
      return <AboutPage onLogin={onLogin} onSignup={onSignup} />;
    case "privacy":
      return <LegalPage doc={PRIVACY} onLogin={onLogin} onSignup={onSignup} />;
    case "terms":
      return <LegalPage doc={TERMS} onLogin={onLogin} onSignup={onSignup} />;
    case "security":
      return <LegalPage doc={SECURITY} onLogin={onLogin} onSignup={onSignup} />;
    default:
      return <FocusLanding onSignup={onSignup} onLogin={onLogin} />;
  }
}
