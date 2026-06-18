import { useState } from "react";
import { API_BASE_URL } from "../../api/client";
import { ConsentModal } from "./ConsentModal";
import { FocusLanding } from "./variants/FocusLanding";

const loginUrl = `${API_BASE_URL}/auth/google/login`;

export function LandingPage() {
  const [showConsent, setShowConsent] = useState(false);

  return (
    <>
      <FocusLanding onLogin={() => setShowConsent(true)} />
      {showConsent && <ConsentModal loginUrl={loginUrl} onClose={() => setShowConsent(false)} />}
    </>
  );
}
