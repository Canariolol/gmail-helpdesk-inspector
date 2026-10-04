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
            <strong>Configura y analiza.</strong> En <em>Configuración</em>, define quiénes solicitan atención, las direcciones
            del equipo y tus criterios de solicitud válida. En <em>Resumen</em>, elige fechas, horario y carpetas. Pulsa <em>Analizar</em> para
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
            <strong>Revisa manualmente.</strong> En <em>Revisión manual</em> encontrarás las conversaciones donde Mira necesita tu confirmación.
            Corrige la clasificación, marca si fue respondido y guarda la revisión: las métricas se recalculan.
          </li>
          <li>
            <strong>Genera reportes.</strong> En <em>Reportes</em> obtienes un consolidado gerencial de varios análisis del
            período: solicitudes sin duplicar reanálisis, tiempos y comparación de ejecuciones.
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
            respuesta humana del equipo dirigida al solicitante en Para o CC. Los intercambios entre miembros del equipo no cuentan como atención al solicitante.
          </li>
          <li><strong>P90 respuesta:</strong> el 90% de las solicitudes se respondió en este tiempo o menos.</li>
          <li><strong>Sin respuesta registrada:</strong> Mira sólo observa los mensajes almacenados en la casilla conectada. Una respuesta guardada en la cuenta personal de un agente o enviada por otro canal puede existir sin aparecer aquí. Confirma que las respuestas de tu equipo se archiven en la casilla auditada; para buzones compartidos de Microsoft 365, tu administrador debe habilitar las copias de enviados.</li>
          <li><strong>Hasta último envío:</strong> tiempo corrido entre la recepción y el último envío del equipo dirigido al solicitante. No confirma que la solicitud esté resuelta.</li>
          <li><strong>Pendientes de revisión:</strong> conversaciones donde Mira necesita tu confirmación; pueden tener un estado tentativo mientras esperan revisión.</li>
          <li><strong>Sin revisión pendiente:</strong> proporción de hilos sin revisión pendiente, incluidas las correcciones manuales. No indica la exactitud de la clasificación.</li>
        </ul>
      </section>
      <section className="card">
        <h2>Privacidad y permisos</h2>
        <PrivacyCallout />
        <ul className="help-list plain">
          <li>En Google y Microsoft solicitamos permisos de lectura del correo. Por IMAP, Mira realiza operaciones de lectura; los permisos de la credencial los define tu proveedor.</li>
          <li>Mira no envía, etiqueta, archiva, edita ni elimina correos.</li>
          <li>Los cuerpos completos se usan solo durante el análisis/auditoría y no se conservan como datos de producto.</li>
          <li>
            La revisión IA se habilita con tu autorización. Puedes desactivarla desde
            <em> Configuración</em>; cada candidato que requiera evaluar tus criterios de atención pasará a revisión manual y el cambio se aplicará al próximo análisis.
          </li>
          <li>Las métricas guardan trazabilidad: puedes revisar qué hilos componen cada número.</li>
        </ul>
      </section>
    </div>
  );
}

const classificationDescriptions: Record<Classification, string> = {
  valid_client_request: "Solicitud de una persona del público configurado que requiere atención del equipo.",
  internal: "Conversación sin solicitante, entre personas del equipo que responde.",
  automated: "Mensaje generado automáticamente (confirmaciones, avisos de sistema).",
  newsletter: "Boletines y correos de marketing.",
  spam: "Correo no deseado.",
  misc: "Correo ignorado según los filtros configurados.",
  ambiguous: "Mira necesita una revisión manual para confirmar esta clasificación.",
};
