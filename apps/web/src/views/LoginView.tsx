import { LogIn } from "lucide-react";
import { API_BASE_URL } from "../api/client";

export function LoginView() {
  return (
    <main className="login-screen">
      <div className="login-card">
        <img src="/logo-192.png" alt="" />
        <h1>Gmail Helpdesk Inspector</h1>
        <p>Audita tu casilla de Gmail.</p>
        <a className="btn-primary" href={`${API_BASE_URL}/auth/google/login`}>
          <LogIn size={18} />
          Conéctate, preciosura =*
        </a>
      </div>
    </main>
  );
}
