import { CreditCard, Globe, Lock } from "lucide-react";

/** Moneda de cobro y alcance de la lectura de correo. */
export function PaymentNotes() {
  return (
    <ul className="access-notes" aria-label="Cómo funcionan los pagos y la conexión de la casilla">
      <li>
        <CreditCard size={15} /> Los pagos se procesan en <strong>CLP por Mercado Pago</strong>.
      </li>
      <li>
        <Globe size={15} /> Los valores en USD son solo <strong>referencia internacional</strong>.
      </li>
      <li>
        <Lock size={15} /> Mira realiza únicamente operaciones de <strong>lectura</strong> en tu casilla.
        Los permisos de una contraseña IMAP dependen de tu proveedor.
      </li>
    </ul>
  );
}
