import { Loader2 } from "lucide-react";
import { AccessShell } from "./AccessShell";

interface LoadingGateProps {
  title?: string;
  body?: string;
}

export function LoadingGate({
  title = "Cargando tu cuenta",
  body = "Estamos validando tu sesión.",
}: LoadingGateProps) {
  return (
    <AccessShell>
      <div className="access-head access-head-center">
        <span className="access-spinner" aria-hidden="true">
          <Loader2 size={26} />
        </span>
        <h1>{title}</h1>
        <p className="access-lede">{body}</p>
      </div>
    </AccessShell>
  );
}
