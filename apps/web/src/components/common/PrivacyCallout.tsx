import { CheckCircle2, LockKeyhole, ShieldCheck } from "lucide-react";

type Props = {
  compact?: boolean;
};

export function PrivacyCallout({ compact = false }: Props) {
  return (
    <section className={compact ? "privacy-callout compact" : "privacy-callout"} aria-label="Privacidad y permisos de Gmail">
      <div className="privacy-callout-icon" aria-hidden="true">
        <ShieldCheck size={22} />
      </div>
      <div>
        <h2>Acceso Gmail de solo lectura</h2>
        <p>
          La app solo puede leer los correos necesarios para el análisis: no puede enviar, editar, etiquetar ni borrar
          mensajes.
        </p>
        <ul>
          <li>
            <LockKeyhole size={15} /> Tu acceso se almacena cifrado.
          </li>
          <li>
            <CheckCircle2 size={15} /> Métricas auditables hasta los hilos que las originan.
          </li>
          <li>
            <CheckCircle2 size={15} /> No conservamos cuerpos completos de correos.
          </li>
        </ul>
      </div>
    </section>
  );
}
