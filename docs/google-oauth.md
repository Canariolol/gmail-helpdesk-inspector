# Configuración de Google OAuth

Mira pide únicamente `https://www.googleapis.com/auth/gmail.readonly` para leer
correo y configuración de la casilla. Este permiso es de lectura, pero Google
lo clasifica como **restringido**. La disponibilidad de Google en
`/mailbox/providers` no acredita aprobación de la aplicación ni del scope.

## Configuración del cliente

1. Crear o revisar el proyecto y cliente web de OAuth en Google Cloud.
2. En Google Auth Platform, revisar Branding, Audience y Data Access. Declarar
   `gmail.readonly` y configurar las páginas públicas de la aplicación y privacidad.
3. Registrar la URL de callback que coincida exactamente con
   `GOOGLE_REDIRECT_URL`. Desarrollo local usa
   `http://localhost:8080/gmail/connect/callback`; producción usa
   `https://mira.ninfasolutions.com/gmail/connect/callback`.
4. Configurar el Client ID y secreto en el entorno correspondiente. En producción,
   usar el gestor de secretos y mantenerlos fuera del repositorio.

## Condiciones para distribución pública

Una aplicación pública que pide `gmail.readonly` debe completar la verificación
del scope restringido, salvo que califique para una excepción documentada por
Google. Mira recupera y procesa contenido de correo en servidores. Por eso,
además de la verificación, el modelo SaaS público requiere una evaluación de
seguridad por un evaluador aprobado por Google y su renovación al menos cada
12 meses. Esto comprende el acceso, almacenamiento y transmisión de los datos
restringidos, incluidos los proveedores que participan en su procesamiento.

La publicación de la web, el permiso de sólo lectura y los tests de software no
sustituyen esos requisitos. La excepción de uso interno corresponde a una
aplicación de la propia organización de Google Workspace, con audiencia
Internal; no cubre automáticamente un SaaS que acepta tenants independientes.

Antes de abrir el registro a usuarios externos, comprobar en la consola:

- Branding publicado, dominios autorizados verificados y páginas públicas de
  inicio y privacidad coherentes con el uso real del correo.
- Aprobación de Data Access para `gmail.readonly` y cumplimiento de la política
  de uso de datos de Google, incluyendo el procesamiento en servidores.
- Evaluación de seguridad vigente y fecha de renovación, o evidencia de una
  excepción que realmente corresponda a la modalidad de distribución.

La revisión del 6 de octubre de 2026 verificó estos requisitos en documentación
oficial. **No inspeccionó el estado actual de aprobación, la evaluación CASA ni
la audiencia del proyecto en la consola.** No se debe afirmar que están aprobados,
rechazados o en Testing a partir del código o de `/mailbox/providers`.

Fuentes: [scopes de Gmail](https://developers.google.com/workspace/gmail/api/auth/scopes),
[verificación de scopes restringidos y evaluación de seguridad](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification).

## Uso de pruebas y `access_denied`

Si la consola muestra audiencia External y estado Testing, sólo pueden conectar
las cuentas incluidas como test users. Para `gmail.readonly`, Google entrega
refresh tokens que vencen a los siete días en ese estado. Revisar esa condición
cuando un análisis programado exige reconexiones frecuentes. No se ha confirmado
que el proyecto desplegado siga en Testing.

Un `403: access_denied` requiere revisar el motivo que muestra Google y la
configuración del proyecto; también pueden intervenir las políticas del
administrador de Workspace. Si el motivo es que una cuenta no es test user,
agregarla en Audience mientras se mantenga la fase de pruebas. Agregar test users
no completa la verificación necesaria para distribución pública.

Fuente: [condiciones de vencimiento y revocación de refresh tokens](https://developers.google.com/identity/protocols/oauth2#expiration).
