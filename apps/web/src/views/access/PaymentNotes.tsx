import { CreditCard, Globe, Lock } from "lucide-react";

/** Copy obligatorio para Chile: CLP por Mercado Pago, USD referencial, Gmail readonly post-pago. */
export function PaymentNotes() {
  return (
    <ul className="access-notes" aria-label="Cómo funcionan los pagos y la conexión de Gmail">
      <li>
        <CreditCard size={15} /> Los pagos se procesan en <strong>CLP por Mercado Pago</strong>.
      </li>
      <li>
        <Globe size={15} /> Los valores en USD son solo <strong>referencia internacional</strong>.
      </li>
      <li>
        <Lock size={15} /> Gmail se conecta después del pago o trial, y solo con permiso de{" "}
        <strong>lectura</strong>.
      </li>
    </ul>
  );
}
