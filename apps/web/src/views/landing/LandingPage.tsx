import { API_BASE_URL } from "../../api/client";
import { FocusLanding } from "./variants/FocusLanding";

const loginUrl = `${API_BASE_URL}/auth/workos/login`;

export function LandingPage() {
  return (
    <FocusLanding
      onLogin={() => {
        window.location.href = loginUrl;
      }}
    />
  );
}
