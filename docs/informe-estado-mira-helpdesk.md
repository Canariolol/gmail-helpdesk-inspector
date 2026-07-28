# Informe de estado — Mira Helpdesk

**Corte de evidencia:** 22 de julio de 2026.  
**Fuente:** repositorio y documentación interna del proyecto. Este informe no
afirma el estado de servicios externos que no fue comprobado durante este corte.

## Qué es Mira Helpdesk

Mira Helpdesk es una aplicación web para auditar una casilla de correo usada
como mesa de ayuda. No reemplaza a un sistema de tickets ni administra el
correo: lee la casilla conectada, identifica conversaciones que parecen
solicitudes de clientes, mide la respuesta del equipo y deja trazabilidad hasta
el hilo que compone cada indicador.

Su propósito es entregar una radiografía operativa de una atención que hoy se
realiza por email: volumen de solicitudes, casos respondidos y pendientes,
tiempo de primera respuesta, clasificaciones dudosas y tendencias del período.
La persona usuaria puede revisar y corregir manualmente los casos ambiguos.

El acceso a Gmail es de solo lectura (`gmail.readonly`). El sistema no envía,
etiqueta, elimina ni modifica mensajes mediante Gmail. Durante el análisis usa
reglas y heurísticas; la auditoría con IA a través de Amazon Bedrock es
configurable y puede desactivarse. Los cuerpos completos se consultan para el
análisis, pero la persistencia prevista se limita a datos derivados y metadatos
necesarios para la auditoría.

La aplicación incluye autenticación, configuración por organización, conexión
de casilla, panel de métricas, detalle de hilos, revisiones manuales, historial
de ejecuciones, reportes programados y controles básicos de privacidad. La
persistencia de ejecución usa PostgreSQL/Supabase y la arquitectura está
separada en interfaz React, API Rust y worker de IA Python.

## Fase de desarrollo

**Fase actual: pre-lanzamiento controlado / validación final de la primera
versión comercial.**

El producto no está en una fase de prototipo: su flujo principal está
implementado, tiene pruebas automatizadas y cuenta con guías de despliegue,
operación, rollback, respaldo y observabilidad. Sin embargo, tampoco debe
considerarse listo para cobrar o abrirse sin restricciones: el checklist maestro
mantiene bloqueadores explícitos para ese lanzamiento.

El alcance de lanzamiento documentado sigue siendo Gmail y Google Workspace,
con una casilla por organización. El soporte para Outlook/Microsoft 365 ya está
implementado en código, pero no está habilitado operativamente: falta registrar
la aplicación en Azure AD, cargar sus credenciales y realizar una conexión y
análisis reales. IMAP continúa como una fase posterior.

## Evidencia de avance

- El chequeo completo del repositorio, ejecutado el 22 de julio de 2026, pasó:
  189 pruebas Rust, 11 pruebas del worker de IA, auditoría de dependencias web
  sin vulnerabilidades y compilación de producción de la interfaz.
- El flujo funcional central existe: conexión de casilla, análisis, clasificación
  auditable, métricas, revisión humana, reportes y programación diaria.
- La base de datos ya fue migrada a PostgreSQL/Supabase y el repositorio contiene
  controles de sesión, desconexión de casilla, borrado de datos de análisis,
  límites de uso, billing y observabilidad.
- La integración multiproveedor fue neutralizada; Gmail funciona como proveedor
  actual y Microsoft Graph cuenta con adaptador y rutas de conexión pendientes
  de configuración externa.

## Pendientes que impiden declararlo listo para lanzamiento pagado

1. Completar y probar de punta a punta Mercado Pago: checkout embebido, webhook
   autenticado y flujo de pago real.
2. Activar y validar `APP_ENV=production` en el despliegue objetivo.
3. Publicar la política de privacidad y los términos y condiciones tras su
   revisión correspondiente.
4. Ejecutar el smoke test final de producción.
5. Resolver las decisiones de operación aún abiertas, incluida la eliminación
   completa de cuenta y el borrado automático por retención.

## Conclusión

Mira Helpdesk es una primera versión funcional y técnicamente avanzada para
medir la atención por correo, con preparación operativa en curso. La etapa
correcta no es desarrollo inicial ni operación comercial plenamente liberada:
es una **validación pre-lanzamiento controlada**, enfocada en cerrar pagos,
requisitos legales y comprobaciones reales de producción antes de admitir
cobros o ampliar el acceso.

## Referencias internas

- [Descripción técnica y alcance actual](../README.md)
- [Especificación funcional original](../specs_prd.md)
- [Checklist maestro de preparación para producción](plan-production-ready.md)
- [Bitácora de evidencia de preparación](production-readiness-log.md)
- [Plan y estado multiproveedor](plan-conexion-multiproveedor.md)
- [Runbook de beta privada y riesgos conocidos](deployment-beta-runbook.md)
