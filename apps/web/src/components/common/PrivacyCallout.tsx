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
          Usamos el scope <code>gmail.readonly</code>: la app no puede enviar, editar, etiquetar ni borrar correos.
        </p>
        <ul>
          <li>
            <LockKeyhole size={15} /> Tokens almacenados cifrados.
          </li>
          <li>
            <CheckCircle2 size={15} /> Métricas auditables hasta los hilos que las originan.
          </li>
          <li>
            <CheckCircle2 size={15} /> Sin persistencia de cuerpos completos en Firestore.
          </li>
        </ul>
      </div>
    </section>
  );
}
