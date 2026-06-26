// Contenido LEGAL EN BORRADOR — estructura + texto de relleno (placeholders).
// Pendiente de redacción y revisión por un abogado antes de publicar.

export type LegalSection = { heading: string; body: string };

export type LegalDoc = {
  title: string;
  updated: string;
  intro: string;
  sections: LegalSection[];
};

const PLACEHOLDER = "Contenido de ejemplo pendiente de redacción legal.";

export const PRIVACY: LegalDoc = {
  title: "Política de privacidad",
  updated: "Borrador",
  intro: `${PLACEHOLDER} Esta política describirá cómo Mira Helpdesk trata los datos personales conforme a la Ley N° 19.628 sobre protección de la vida privada (Chile).`,
  sections: [
    { heading: "Responsable del tratamiento", body: `${PLACEHOLDER} Identificación de la empresa responsable y datos de contacto.` },
    { heading: "Datos que recopilamos", body: `${PLACEHOLDER} Datos de cuenta, metadatos de correos y métricas derivadas; no se almacena el cuerpo completo de los correos.` },
    { heading: "Acceso a Gmail (solo lectura)", body: `${PLACEHOLDER} Alcance del permiso de solo lectura: no enviamos, no etiquetamos, no archivamos ni borramos correos.` },
    { heading: "Finalidad del tratamiento", body: `${PLACEHOLDER} Para qué se usan los datos: generación de métricas, reportes y auditoría con IA opcional.` },
    { heading: "Conservación y retención", body: `${PLACEHOLDER} Plazos de retención por plan y criterios de eliminación.` },
    { heading: "Encargados y terceros", body: `${PLACEHOLDER} Proveedores que tratan datos por encargo: MercadoPago (pagos), WorkOS (autenticación) y el proveedor de IA.` },
    { heading: "Derechos del titular", body: `${PLACEHOLDER} Derechos de acceso, rectificación, cancelación y oposición conforme a la Ley 19.628, y cómo ejercerlos.` },
    { heading: "Seguridad de los datos", body: `${PLACEHOLDER} Medidas técnicas y organizativas; ver también la página de Seguridad.` },
    { heading: "Cambios a esta política", body: `${PLACEHOLDER} Cómo se comunican las actualizaciones de esta política.` },
    { heading: "Contacto", body: `${PLACEHOLDER} Canal de contacto para consultas de privacidad.` },
  ],
};

export const TERMS: LegalDoc = {
  title: "Términos de uso y condiciones",
  updated: "Borrador",
  intro: `${PLACEHOLDER} Estos términos regularán el uso de Mira Helpdesk.`,
  sections: [
    { heading: "Aceptación de los términos", body: `${PLACEHOLDER} Al crear una cuenta el usuario acepta estos términos.` },
    { heading: "Descripción del servicio", body: `${PLACEHOLDER} Mira Helpdesk mide la actividad de una casilla de Gmail y entrega métricas y reportes.` },
    { heading: "Cuenta y registro", body: `${PLACEHOLDER} Requisitos de la cuenta y responsabilidad sobre las credenciales.` },
    { heading: "Planes, precios y pagos", body: `${PLACEHOLDER} Los cobros se realizan en pesos chilenos (CLP) a través de MercadoPago; los valores en USD son solo referencia.` },
    { heading: "Suscripciones, renovación y cancelación", body: `${PLACEHOLDER} Renovación automática, periodos de prueba y cancelación con acceso hasta fin de periodo.` },
    { heading: "Uso aceptable", body: `${PLACEHOLDER} Conductas permitidas y prohibidas.` },
    { heading: "Propiedad intelectual", body: `${PLACEHOLDER} Titularidad del software y de las marcas.` },
    { heading: "Limitación de responsabilidad", body: `${PLACEHOLDER} Alcance y límites de responsabilidad del servicio.` },
    { heading: "Terminación", body: `${PLACEHOLDER} Causales de suspensión o término de la cuenta.` },
    { heading: "Ley aplicable y jurisdicción", body: `${PLACEHOLDER} Estos términos se rigen por las leyes de Chile y la jurisdicción de sus tribunales.` },
    { heading: "Contacto", body: `${PLACEHOLDER} Canal de contacto para consultas sobre los términos.` },
  ],
};

export const SECURITY: LegalDoc = {
  title: "Política de seguridad",
  updated: "Borrador",
  intro: `${PLACEHOLDER} Resumen de las prácticas de seguridad de Mira Helpdesk.`,
  sections: [
    { heading: "Alcance de los permisos", body: `${PLACEHOLDER} Acceso de solo lectura a Gmail; el menor privilegio necesario para medir.` },
    { heading: "Cifrado en tránsito y en reposo", body: `${PLACEHOLDER} Uso de HTTPS y cifrado de datos almacenados.` },
    { heading: "Manejo de datos sensibles", body: `${PLACEHOLDER} No se almacena el cuerpo completo de los correos; solo metadatos y métricas.` },
    { heading: "Pagos", body: `${PLACEHOLDER} No almacenamos datos de tarjeta: MercadoPago tokeniza la tarjeta y procesa el cobro.` },
    { heading: "Control de acceso", body: `${PLACEHOLDER} Autenticación vía WorkOS y control de acceso por organización.` },
    { heading: "Retención y eliminación", body: `${PLACEHOLDER} Plazos de retención y borrado de datos.` },
    { heading: "Respuesta a incidentes", body: `${PLACEHOLDER} Procedimiento ante incidentes de seguridad.` },
    { heading: "Reporte de vulnerabilidades", body: `${PLACEHOLDER} Cómo reportar una vulnerabilidad de forma responsable.` },
    { heading: "Contacto de seguridad", body: `${PLACEHOLDER} Canal de contacto del equipo de seguridad.` },
  ],
};
