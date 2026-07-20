import type { Classification } from "../api/types";
import { PrivacyCallout } from "../components/common/PrivacyCallout";
import { StatusBadge } from "../components/common/StatusBadge";

const classifications: Classification[] = [
  "valid_client_request",
  "internal",
  "automated",
  "newsletter",
  "spam",
  "misc",
  "ambiguous",
];

export function AyudaView() {
  return (
    <div className="view ayuda">
      <section className="card">
        <h2>Cómo usar Mira</h2>
        <ol className="help-steps">
          <li>
            <strong>Configura y analiza.</strong> En <em>Resumen</em>, define el rango de fechas y horario, los dominios
            internos de tu empresa y lo que quieres ignorar (dominios y palabras clave). Pulsa <em>Analizar</em> para
            crear e iniciar el análisis de la casilla.
          </li>
          <li>
            <strong>Sigue el progreso.</strong> El banner muestra el avance y los hilos procesados.
            Las métricas y gráficos se actualizan automáticamente cada pocos segundos.
          </li>
          <li>
            <strong>Audita los hilos.</strong> En <em>Hilos</em> puedes ver cada conversación con su recepción, primera
            respuesta y último envío. Haz clic en un hilo para ver la línea de tiempo de mensajes y sus razones de
            clasificación.
          </li>
          <li>
            <strong>Revisa manualmente.</strong> En <em>Revisión manual</em> encontrarás los hilos donde la IA tuvo dudas.
            Corrige la clasificación, marca si fue respondido y guarda la revisión: las métricas se recalculan.
          </li>
          <li>
            <strong>Genera reportes.</strong> En <em>Reportes</em> obtienes un consolidado gerencial de varios análisis en
            un rango de tiempo: totales, promedios ponderados y tendencias.
          </li>
        </ol>
      </section>
      <section className="card">
        <h2>Clasificaciones</h2>
        <ul className="help-list">
          {classifications.map((classification) => (
            <li key={classification}>
              <StatusBadge classification={classification} />
              <span>{classificationDescriptions[classification]}</span>
            </li>
          ))}
        </ul>
      </section>
      <section className="card">
        <h2>Métricas</h2>
        <ul className="help-list plain">
          <li>
            <strong>T. medio respuesta:</strong> promedio entre la recepción del primer mensaje del cliente y la primera
            respuesta interna.
          </li>
          <li><strong>P90 respuesta:</strong> el 90% de las solicitudes se respondió en este tiempo o menos.</li>
          <li><strong>Cierre medio:</strong> promedio entre la recepción y el último envío interno del hilo.</li>
          <li><strong>Pendientes de revisión:</strong> hilos donde reglas o IA requieren confirmación humana; pueden tener un estado tentativo mientras esperan revisión.</li>
          <li><strong>Confianza:</strong> proporción de hilos que no están pendientes de revisión manual.</li>
        </ul>
      </section>
      <section className="card">
        <h2>Privacidad y permisos</h2>
        <PrivacyCallout />
        <ul className="help-list plain">
          <li>El permiso sobre tu casilla es exclusivamente de solo lectura.</li>
          <li>Mira no envía, etiqueta, archiva, edita ni elimina correos.</li>
          <li>Los cuerpos completos se usan solo durante el análisis/auditoría y no se conservan como datos de producto.</li>
          <li>
            La auditoría IA viene activa para mejorar la clasificación. Puedes desactivarla desde
            <em> Configuración</em>; los casos inciertos pasarán a revisión manual y el cambio se aplicará al próximo análisis.
          </li>
          <li>Las métricas guardan trazabilidad: puedes revisar qué hilos componen cada número.</li>
        </ul>
      </section>
    </div>
  );
}

const classificationDescriptions: Record<Classification, string> = {
  valid_client_request: "Solicitud real de un cliente que requiere atención del equipo.",
  internal: "Conversación entre miembros del equipo interno.",
  automated: "Mensaje generado automáticamente (confirmaciones, avisos de sistema).",
  newsletter: "Boletines y correos de marketing.",
  spam: "Correo no deseado.",
  misc: "Correo ignorado según los filtros configurados.",
  ambiguous: "La IA no pudo clasificarlo con certeza; requiere revisión manual.",
};
