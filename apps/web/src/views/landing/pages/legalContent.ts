// Contenido LEGAL EN BORRADOR — redacción completa pendiente de revisión por
// un abogado antes de publicar como vigente. Mantener `updated: "Borrador"`
// hasta esa revisión. Datos pendientes de confirmar: RUT/razón social exacta
// de Ninfa Solutions y correo de contacto definitivo.

export type LegalSection = { heading: string; body: string };

export type LegalDoc = {
  title: string;
  updated: string;
  intro: string;
  sections: LegalSection[];
};

export const CONTACTO = "soporte@ninfasolutions.com";

export const PRIVACY: LegalDoc = {
  title: "Política de privacidad",
  updated: "Borrador",
  intro:
    "Esta política describe cómo Mira Helpdesk trata los datos personales de las personas usuarias y de los correos analizados, conforme a la Ley N° 19.628 sobre protección de la vida privada (Chile) y su normativa sucesora.",
  sections: [
    {
      heading: "Responsable del tratamiento",
      body: `El responsable del tratamiento es Ninfa Solutions (Chile). Para consultas sobre esta política o sobre tus datos, escribe a ${CONTACTO}.`,
    },
    {
      heading: "Datos que recopilamos",
      body: "De tu cuenta: nombre, correo y organización, gestionados a través de nuestro proveedor de autenticación. De la casilla conectada: identificadores de hilos y mensajes, fechas, participantes, asuntos, fragmentos breves (snippets), decisiones de clasificación, resultados de auditoría y métricas agregadas. No almacenamos el cuerpo completo de los correos: se procesa temporalmente durante el análisis y se descarta.",
    },
    {
      heading: "Acceso a la casilla (solo lectura)",
      body: "Para leer correo mediante OAuth solicitamos gmail.readonly en Google y Mail.Read en Microsoft; los buzones compartidos de Microsoft 365 también requieren Mail.Read.Shared. En conexiones IMAP, Mira realiza operaciones de lectura sobre TLS, pero los permisos de la contraseña dependen del proveedor: recomendamos una contraseña de aplicación cuando esté disponible. No enviamos, respondemos, etiquetamos, archivamos ni eliminamos correos. Las credenciales se guardan cifradas y puedes desconectar la casilla; intentamos revocar el acceso OAuth ante el proveedor cuando es posible. Una contraseña IMAP debe revocarse desde tu proveedor.",
    },
    {
      heading: "Finalidad del tratamiento",
      body: "Usamos los datos para generar métricas de atención de la casilla conectada, reportes periódicos y una auditoría asistida por inteligencia artificial que clasifica casos ambiguos para revisión humana. No usamos los datos para publicidad ni los vendemos a terceros.",
    },
    {
      heading: "Auditoría con inteligencia artificial",
      body: "La auditoría IA se habilita cuando la persona propietaria la autoriza en Configuración. Puede desactivarse y el cambio aplica al próximo análisis. Al proveedor de IA (Amazon Bedrock) se envían participantes, fecha, asunto y un texto limitado por mensaje; no se envían adjuntos, imágenes ni encabezados completos. Un mensaje corto puede caber completo dentro del límite de texto, por lo que fragmentos con información sensible podrían ser procesados.",
    },
    {
      heading: "Conservación y eliminación",
      body: "Cada análisis conserva el plazo de retención vigente cuando se creó; los cambios de política aplican a los próximos análisis. Los registros anteriores sin fecha ni política de retención usan un plazo de 90 días desde su creación. Al vencer dejan de ser accesibles y el mantenimiento programado elimina sus registros cuando está habilitado en el despliegue. Puedes borrar tus análisis y solicitar la eliminación completa de tu cuenta por el canal de contacto, verificando tu identidad. Los respaldos pueden conservar copias hasta que venza el plazo establecido por su operador.",
    },
    {
      heading: "Encargados y subprocesadores",
      body: "Los servicios integrados incluyen WorkOS (autenticación y sesiones), Google y Microsoft (acceso OAuth al correo), el proveedor de correo configurado por IMAP, Google Cloud (infraestructura), Supabase/PostgreSQL (base de datos según el despliegue), Amazon Web Services — Bedrock (auditoría IA) y Resend (reportes por correo). Mercado Pago procesa los pagos habilitados; Mira no recibe ni almacena números de tarjeta. El operador debe mantener la lista de proveedores aplicable a su despliegue.",
    },
    {
      heading: "Transferencias internacionales",
      body: "La ubicación de alojamiento y procesamiento depende de la configuración del despliegue y de los proveedores utilizados. El operador debe informar las regiones y las condiciones de las transferencias internacionales aplicables antes de ofrecer el servicio público.",
    },
    {
      heading: "Derechos del titular",
      body: `Puedes solicitar acceso, rectificación, cancelación u oposición al tratamiento de tus datos escribiendo a ${CONTACTO}. Responderemos dentro de un plazo razonable y verificaremos tu identidad antes de ejecutar solicitudes que afecten datos.`,
    },
    {
      heading: "Seguridad de los datos",
      body: "Todo el tráfico usa HTTPS, las credenciales de la casilla conectada se cifran antes de guardarse, el componente de IA no es accesible públicamente y el acceso a los datos está aislado por organización. El detalle está en la página de Seguridad.",
    },
    {
      heading: "Cambios a esta política",
      body: "Publicaremos aquí las versiones nuevas con su fecha. Si un cambio es material, lo comunicaremos dentro de la aplicación y pediremos tu aceptación cuando corresponda.",
    },
    {
      heading: "Contacto",
      body: `Para cualquier consulta de privacidad: ${CONTACTO}.`,
    },
  ],
};

export const TERMS: LegalDoc = {
  title: "Términos de uso y condiciones",
  updated: "Borrador",
  intro:
    "Estos términos regulan el uso de Mira Helpdesk, un servicio de Ninfa Solutions (Chile). Al crear una cuenta o usar el servicio aceptas estos términos.",
  sections: [
    {
      heading: "Descripción del servicio",
      body: "Mira analiza la actividad de una casilla de correo conectada por la organización y entrega métricas de atención, reportes y una auditoría asistida por IA orientada a revisión humana. La versión actual admite Gmail, Google Workspace, Outlook, Microsoft 365 y proveedores con IMAP sobre TLS, con una casilla y una cuenta propietaria por organización. Las métricas reflejan las respuestas observadas en la casilla conectada; no prueban la resolución de una solicitud ni la atención realizada por otros canales.",
    },
    {
      heading: "Cuenta y registro",
      body: "Necesitas una cuenta creada mediante nuestro proveedor de autenticación. Eres responsable de mantener la confidencialidad de tus credenciales y de la actividad realizada con tu cuenta.",
    },
    {
      heading: "Autorización sobre la casilla conectada",
      body: "Quien conecta una casilla declara contar con autorización para conectarla y para que Mira lea esos correos. Mira no envía desde la casilla conectada ni modifica sus correos; los reportes se envían mediante un servicio separado. En conexiones IMAP, quien conecta la casilla es responsable de gestionar y revocar la contraseña entregada al proveedor.",
    },
    {
      heading: "Resultados de la IA y revisión humana",
      body: "Las clasificaciones y auditorías generadas con IA son apoyo para la gestión y pueden contener errores. No deben usarse como única base para decisiones laborales o disciplinarias; el servicio está diseñado para que los casos dudosos pasen por revisión humana.",
    },
    {
      heading: "Planes, precios y pagos",
      body: "Mira Free permite comenzar sin tarjeta, con los límites indicados en la cuenta. Los planes de pago publicados incluyen sus precios, ciclo de facturación y cupos; los análisis manuales y programados comparten los cupos mensuales. Los pagos habilitados se procesan en pesos chilenos a través de Mercado Pago. Antes de confirmar una suscripción se muestra el importe y la renovación automática. Puedes cancelar la renovación desde la aplicación; el acceso adquirido se conserva hasta el término de su período. Estas condiciones siguen pendientes de revisión legal, como el resto de este documento.",
    },
    {
      heading: "Uso aceptable",
      body: "No puedes usar Mira para vigilar personas sin autorización de tu organización, intentar acceder a datos de otras organizaciones, revender el servicio sin acuerdo escrito, ni interferir con su operación o seguridad.",
    },
    {
      heading: "Propiedad intelectual",
      body: "El software, la marca y los materiales de Mira pertenecen a Ninfa Solutions. Los datos de tu casilla y los resultados derivados de ellos pertenecen a tu organización.",
    },
    {
      heading: "Disponibilidad y limitación de responsabilidad",
      body: "El servicio se ofrece «tal cual», sin garantía de disponibilidad ininterrumpida. En la máxima medida permitida por la ley, la responsabilidad total de Ninfa Solutions se limita a los montos pagados por el servicio en los últimos doce meses; mientras el servicio no tenga cobros habilitados, la responsabilidad se limita a lo que la ley chilena no permita excluir.",
    },
    {
      heading: "Suspensión y terminación",
      body: "Podemos suspender o terminar cuentas que incumplan estos términos o comprometan la seguridad del servicio, avisando cuando sea razonable. Puedes dejar de usar el servicio y solicitar la eliminación de tus datos en cualquier momento.",
    },
    {
      heading: "Eliminación de datos",
      body: "Puedes borrar tus análisis desde la aplicación y solicitar la eliminación completa de la cuenta por el canal de contacto. Conservaremos solo los registros que una obligación legal o contable exija mantener.",
    },
    {
      heading: "Cambios a estos términos",
      body: "Publicaremos aquí las versiones nuevas con su fecha. Los cambios materiales se comunicarán dentro de la aplicación y, cuando corresponda, pediremos aceptación nuevamente.",
    },
    {
      heading: "Ley aplicable y jurisdicción",
      body: `Estos términos se rigen por las leyes de la República de Chile y cualquier controversia se someterá a sus tribunales competentes. Contacto: ${CONTACTO}.`,
    },
  ],
};

export const SECURITY: LegalDoc = {
  title: "Seguridad",
  updated: "Borrador",
  intro:
    "Resumen honesto de las prácticas de seguridad de Mira Helpdesk. No afirmamos certificaciones que no tenemos ni seguridad absoluta; describimos lo que efectivamente hacemos.",
  sections: [
    {
      heading: "Permiso mínimo sobre la casilla",
      body: "El acceso OAuth al correo utiliza gmail.readonly en Google, Mail.Read en Microsoft y Mail.Read.Shared para sus buzones compartidos. En IMAP se usa TLS y operaciones de lectura; la contraseña puede tener permisos más amplios según el proveedor. Mira no envía, modifica, etiqueta ni borra correos.",
    },
    {
      heading: "Cifrado",
      body: "Todo el tráfico viaja por HTTPS. Los tokens de acceso a la casilla se cifran a nivel de aplicación antes de persistirse, además del cifrado en reposo del proveedor de base de datos.",
    },
    {
      heading: "Minimización de datos",
      body: "No almacenamos cuerpos completos de correos: se procesan temporalmente durante el análisis y se descartan. Persistimos metadatos, fragmentos breves, clasificaciones y métricas.",
    },
    {
      heading: "Inteligencia artificial",
      body: "El componente de IA (Amazon Bedrock) no es accesible públicamente: solo la API de Mira puede invocarlo, autenticada por IAM. Recibe participantes, fecha, asunto y texto limitado por mensaje; nunca adjuntos ni imágenes. La organización puede desactivar la auditoría IA desde Configuración.",
    },
    {
      heading: "Control de acceso",
      body: "La autenticación se delega en WorkOS; las sesiones expiran a los 30 días, se pueden revocar desde la aplicación y el acceso a los datos está aislado por organización.",
    },
    {
      heading: "Pagos",
      body: "Cuando los cobros estén habilitados, Mercado Pago tokenizará la tarjeta en tu navegador: Mira nunca recibe ni almacena números de tarjeta ni códigos de seguridad.",
    },
    {
      heading: "Respaldo y recuperación",
      body: "El operador del despliegue debe configurar respaldos y verificar la restauración. Sus ubicaciones y plazos de conservación dependen de esa configuración. El borrado de datos de la aplicación no elimina de inmediato las copias que puedan permanecer en respaldos.",
    },
    {
      heading: "Retención y eliminación",
      body: "Los análisis vencidos dejan de ser accesibles y el mantenimiento programado elimina sus registros cuando está habilitado en el despliegue. Puedes borrar tus análisis desde la aplicación y solicitar la eliminación de la cuenta por el canal de contacto. Las copias en respaldos siguen el plazo de conservación definido por el operador.",
    },
    {
      heading: "Reporte de vulnerabilidades",
      body: `Si encuentras una vulnerabilidad, escríbenos a ${CONTACTO} con los pasos para reproducirla. Nos comprometemos a responder, no iniciar acciones legales contra reportes de buena fe y coordinar la corrección antes de cualquier divulgación.`,
    },
    {
      heading: "Contacto de seguridad",
      body: `${CONTACTO}`,
    },
  ],
};
