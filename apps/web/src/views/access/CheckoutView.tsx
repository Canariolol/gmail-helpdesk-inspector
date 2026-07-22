import { ArrowLeft, Loader2, Lock } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { BillingInterval, BillingPlan } from "../../api/types";
import { AccessShell } from "./AccessShell";
import { PaymentNotes } from "./PaymentNotes";
import { formatClp } from "./PricingPlans";

const MP_PUBLIC_KEY = import.meta.env.VITE_MERCADOPAGO_PUBLIC_KEY as string | undefined;
const MP_SDK_URL = "https://sdk.mercadopago.com/js/v2";
const BRICK_CONTAINER_ID = "mp-card-brick-container";

export type CardSubmitData = { cardTokenId: string; payerEmail: string };

type Props = {
  plan: BillingPlan;
  billingInterval: BillingInterval;
  onPay: (data: CardSubmitData) => void;
  onBack: () => void;
  isSubmitting: boolean;
  error: string | null;
};

// --- Tipos mínimos del SDK de Mercado Pago (cargado desde su CDN) ---
type CardFormData = { token: string; payer?: { email?: string } };
type BrickController = { unmount?: () => void };
type BricksBuilder = {
  create: (brick: string, containerId: string, settings: unknown) => Promise<BrickController>;
};
type MercadoPagoInstance = { bricks: () => BricksBuilder };
type MercadoPagoConstructor = new (
  publicKey: string,
  options?: { locale?: string },
) => MercadoPagoInstance;

declare global {
  interface Window {
    MercadoPago?: MercadoPagoConstructor;
  }
}

// Carga el SDK v2 una sola vez. La tokenización ocurre en el script de Mercado
// Pago (PCI): los datos de tarjeta nunca tocan nuestro backend.
let sdkPromise: Promise<void> | null = null;
function loadMercadoPagoSdk(): Promise<void> {
  if (window.MercadoPago) return Promise.resolve();
  if (sdkPromise) return sdkPromise;
  sdkPromise = new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = MP_SDK_URL;
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("No se pudo cargar el SDK de Mercado Pago."));
    document.head.appendChild(script);
  });
  return sdkPromise;
}

export function CheckoutView({ plan, billingInterval, onPay, onBack, isSubmitting, error }: Props) {
  const [isReady, setReady] = useState(false);
  const [brickError, setBrickError] = useState<string | null>(null);
  // Ref para no recrear el Brick cuando cambia `onPay` entre renders.
  const onPayRef = useRef(onPay);
  onPayRef.current = onPay;
  const amount = billingInterval === "annual" ? (plan.clp_annual ?? plan.clp_monthly) : plan.clp_monthly;

  useEffect(() => {
    if (!MP_PUBLIC_KEY) return;
    let cancelled = false;
    let controller: BrickController | null = null;

    loadMercadoPagoSdk()
      .then(() => {
        if (cancelled || !window.MercadoPago) return undefined;
        const mp = new window.MercadoPago(MP_PUBLIC_KEY, { locale: "es-CL" });
        return mp.bricks().create("cardPayment", BRICK_CONTAINER_ID, {
          initialization: { amount },
          customization: { paymentMethods: { maxInstallments: 1 } },
          callbacks: {
            onReady: () => {
              if (!cancelled) setReady(true);
            },
            onError: () => {
              if (!cancelled) setBrickError("No se pudo iniciar el formulario de pago.");
            },
            onSubmit: (formData: CardFormData) => {
              onPayRef.current({
                cardTokenId: formData.token,
                payerEmail: formData.payer?.email ?? "",
              });
              return Promise.resolve();
            },
          },
        });
      })
      .then((created) => {
        if (created) controller = created;
      })
      .catch(() => {
        if (!cancelled) setBrickError("No se pudo cargar el SDK de Mercado Pago.");
      });

    return () => {
      cancelled = true;
      controller?.unmount?.();
    };
  }, [amount]);

  const priceLabel = `$${formatClp(amount)}`;
  const periodLabel = billingInterval === "annual" ? "año" : "mes";

  return (
    <AccessShell>
      <button type="button" className="access-text-btn checkout-back" onClick={onBack}>
        <ArrowLeft size={15} /> Volver a los planes
      </button>

      <div className="access-head">
        <span className="access-eyebrow">Checkout · pago seguro</span>
        <h1>Suscríbete al plan {plan.name}</h1>
        <p className="access-lede">
          Pagas dentro de Mira, sin salir a otra página. Mercado Pago procesa tu tarjeta de forma
          segura.
        </p>
      </div>

      <div className="checkout-summary">
        <div>
          <span>Plan {plan.name}</span>
          {plan.trial_days > 0 ? (
            <small>
              {plan.trial_days} días de prueba, luego {priceLabel} CLP/{periodLabel}
            </small>
          ) : (
            <small>Cobro {billingInterval === "annual" ? "anual" : "mensual"}, IVA incluido</small>
          )}
        </div>
        <strong>
          {priceLabel} <em>CLP/{periodLabel}</em>
        </strong>
      </div>

      {!MP_PUBLIC_KEY && (
        <div className="access-error" role="alert">
          El checkout no está configurado: falta la variable VITE_MERCADOPAGO_PUBLIC_KEY.
        </div>
      )}
      {error && (
        <div className="access-error" role="alert">
          {error}
        </div>
      )}
      {brickError && (
        <div className="access-error" role="alert">
          {brickError}
        </div>
      )}

      <div className="checkout-brick">
        {MP_PUBLIC_KEY && !isReady && !brickError && (
          <p className="checkout-loading">
            <Loader2 size={16} className="spin" /> Cargando formulario seguro…
          </p>
        )}
        <div id={BRICK_CONTAINER_ID} />
        {isSubmitting && (
          <div className="checkout-overlay">
            <Loader2 size={18} className="spin" /> Procesando tu suscripción…
          </div>
        )}
      </div>

      <p className="checkout-secure">
        <Lock size={13} /> Tus datos de tarjeta se cifran y procesan en Mercado Pago. Nunca pasan por
        nuestros servidores.
      </p>

      <PaymentNotes />
    </AccessShell>
  );
}
