# Historial de Versiones (Lokus)

## [5.2.0] - 2026-11-23

### Corrección de errores (Bugs)
- **Estado real en la tabla de precisión de IA:** Se corrigió la tabla de precisión por campo que antes mostraba siempre `Estable` en verde aunque un campo tuviera `0` evaluaciones o precisión baja. Ahora refleja el estado real (`Sin datos`, `Aprendiendo`, `En revisión`, `Baja`, `Mejorable` o `Confiable`) y muestra `—` cuando aún no hay evaluaciones.

### Nuevas funciones
- **Usar cualquier juzgado o tipo de audiencia como plantilla rápida:** Cada opción del desplegable de **Juzgado** y **Tipo de Audiencia** (tanto las incluidas en Lokus como las guardadas por el usuario) ahora tiene un botón de copiar para llevar su texto directo a **Otra** y preseleccionar automáticamente el número (por ejemplo, cambiar `2` por `10` sin volver a escribir todo el nombre).
- **Alta rápida desde búsquedas sin coincidencias:** Al buscar un juzgado o tipo de audiencia que aún no está en la lista, aparece un botón directo `Usar "<texto>" en Otra` para pasarlo al campo editable en un clic.

### Mejoras y funciones modificadas
- **Ventana de Inteligencia Artificial más clara y organizada en 2 pestañas:** Se rediseñó la configuración de IA separando **Conexión y ajustes** (proveedor, modelo, clave API y casillas explicadas en lenguaje cotidiano) de **Aprendizaje y memoria** (indicadores principales, explicación de los porcentajes, diferencia clara entre **Recargar datos** vs. **Consolidar perfil del defensor** / **Consolidar reglas generales**, y los datos técnicos avanzados guardados en un acordeón aparte).
- **Alta unificada en "Otra":** Se eliminó el botón duplicado *Agregar nuevo* al final del menú desplegable de Juzgados y Tipos de Audiencia; ahora **Otra** aparece al inicio de la lista con una guía breve que deja claro que puedes usarlo solo para esa audiencia o pulsar **`+`** para guardarlo en tu lista permanente.

## [5.1.0] - 2026-11-22

### Corrección de errores (Bugs)
- **Los archivos Excel descargados ya abren al instante en Windows:** Se eliminó la configuración interna de impresora externa y rutas antiguas de otro equipo que venían guardadas en la plantilla anterior y hacían que Microsoft Excel se quedara congelado hasta 5 minutos al abrir un archivo por primera vez. Ahora las planillas nuevas se crean limpias y, si exportas a Descargas una planilla creada antes de esta actualización, Lokus la limpia automáticamente al copiarla.
- **Arrastrar y soltar (Drag & Drop) reparado en Windows:** Ya puedes arrastrar planillas y carpetas con el ratón directamente en el panel izquierdo en Windows igual que en Linux, soltar un archivo sobre cualquier registro dentro de una carpeta abierta para guardarlo ahí y arrastrar desde subcarpetas sin que la lista salte de posición.

### Nuevas funciones
- **Editar y renombrar planillas Excel dentro de la aplicación:** Al hacer clic derecho sobre cualquier archivo del panel lateral (o en el nuevo botón de lápiz junto al nombre de la planilla activa arriba) ahora aparece **Editar o renombrar planilla**, para cambiar el nombre del archivo, su Defensor Público asignado y su carpeta en un solo paso sin perder respaldos ni estadísticas.
- **Nombre automático para nuevas planillas (`Defensor_Mes_Año`):** En *Nuevo archivo de audiencias*, al elegir o escribir el Defensor Público el sistema sugiere automáticamente el nombre `[NOMBRE_DEFENSOR]_[MES_ACTUAL]_[AÑO_ACTUAL]`. Si prefieres escribir otro nombre manualmente, Lokus lo respeta y deja un enlace rápido **Usar formato automático** por si luego quieres volver al formato estándar.
- **Botón `+` directo para guardar nuevos juzgados y tipos de audiencia:** Cuando la IA o tú dejen Juzgado o Tipo de audiencia en **Otra**, ahora verás un botón **`+`** justo al lado del campo de texto para guardarlo en tu lista permanente y seleccionarlo automáticamente en un clic.

### Mejoras y funciones modificadas
- **Nueva plantilla oficial con mejor presentación visual:** Se actualizó la plantilla Excel oficial con columnas más amplias y el diseño visual preferido, manteniendo exactamente las mismas filas, totales y el logo institucional.
- **Menos escritura al crear un juzgado o tipo nuevo desde el selector:** Si ya hay un texto escrito en *Otra*, al abrir *Agregar nuevo juzgado* o *Agregar nuevo tipo de audiencia* dentro del menú desplegable ese campo aparece prellenado automáticamente.
- **Orden más cómodo al crear un archivo nuevo:** En la ventana *Nuevo archivo de audiencias*, el campo **Defensor Público asignado** ahora aparece primero para completar de inmediato el nombre de la planilla justo debajo.

## [5.0.0] - 2026-11-22

### Novedades principales
- **Generalización inteligente para cualquier tipo de abreviación (`patrones_abreviacion.rs`):** El sistema de autoaprendizaje ahora deduce patrones parametrizados a partir de las abreviaciones previas de cada defensor sin limitarse a un formato fijo:
  - **Patrones numéricos y ordinales (`{N}`) en cualquier posición:** prefijos (`J10`/`J9` → `J4`, `JPM10`/`J.P.M. 03` → `JPM4`, `EPMS 1`/`EPMS-2` → `EPMS 3`), infijos (`J1M`/`J2 M` → `J4M`), sufijos (`1 MPAL`/`2MPAL` → `4 MPAL`), ordinales en español (`P1`/`P2` → `P4` = *Juzgado Cuarto...*, `1ro`/`2do` → `4to`, `1º`/`2°` → `4º`) y transformaciones de estilo (`Juzgado N Circuito Penal` → `Juzgado N Cto Penal`).
  - **Patrones por municipio o tipo de audiencia (`{X}`):** si el defensor usa `JPM Bosconia` y `JPM Curumaní`, deduce `JPM Chimichagua` o `JPM La Jagua de Ibirico`; si usa `AUD ACUSACION` y `AUD IMPUTACION`, deduce `AUD NULIDAD` o `AUD DE PRORROGA` respetando las tildes oficiales del catálogo.
  - **Siglas fijas cortas (desde 2 letras, con o sin puntos):** aprende equivalencias directas como `JO` / `J.O.` → *Audiencia de juicio oral* o `CG` → *Control de garantías* tanto en juzgados como en tipos de audiencia.
- **Candados de seguridad jurídica y por defensor:** Aislamiento estricto por defensor con herencia desde el Cuaderno General (`*`) solo si no hay contradicciones; exigencia de alta consistencia (no adivina si un defensor mezcla despachos contradictorios bajo la misma sigla); prioridad absoluta a cualquier corrección específica sobre el patrón general; protección de calificadores explícitos (`CTO`, `del Circuito`, `de Garantías`, `Especializado`, `Promiscuo`) y barreras frente a fechas, horas (`4pm`, `15/08/2025`), códigos de fiscalía (`F10`) y radicados.
- **Diagnóstico técnico y exportación enriquecida:** Contador de `Patrones inferidos` en *Configurar IA → Diagnóstico técnico* e inclusión de `patrones_abreviacion` anonimizados en el JSON exportado.

## [4.3.0] - 2026-11-22

### Novedades principales
- **Exportación JSON hasta ~960× más rápida con miles de audiencias:** Rediseño completo del motor de anonimización en pasada lineal (`IndiceRadicados`, `TrieFrases`, `EscudosDefensores` y caché temporal `CacheCatalogoAnon` durante la exportación), reduciendo la exportación con 4.500 audiencias de ~18,3 s a ~19 ms (y ~32 ms con 9.000 audiencias), sin añadir dependencias externas.
- **Privacidad y escudos de defensores reforzados:** Protección mejorada para nombres reales de defensores con iniciales (`Dr. J. Pérez`, `Dra. M.A. Gómez`), honoríficos `Licenciado`/`Licenciada`, nombres compuestos (`Juan-Carlos`, `O'Connor`) y cláusulas con comas; mejor detección de radicados pegados a texto o espaciados sin alterar fechas, teléfonos ni prefijos DANE (`20001`), ni confundir años (`2025`) con nombres de procesados.
- **Validación de alto volumen y 4 nuevas baterías de pruebas:** Cobertura de bordes NFC/NFD, escudos, puntuación y 250 iteraciones de pruebas de propiedades/idempotencia (82/82 tests en Rust).

## [4.2.0] - 2026-11-22

### Novedades principales
- **Menú desplegable editable y borrable de defensores guardados:** Al abrir el selector de Defensor Público tanto en *Nuevo archivo de audiencias* como en *Defensor Público de la planilla*, ahora puedes editar en línea el nombre de cualquier defensor guardado (corrigiendo errores de escritura con migración automática de su aprendizaje) o eliminarlo directamente de tu lista sin salir del formulario.
- **Autoaprendizaje en dos niveles (Defensor + General):** La IA mantiene un cuaderno propio para cada Defensor Público sin mezclar estilos privados entre profesionales, hereda criterios transversales del Cuaderno General (`*`) y promueve automáticamente reglas confirmadas por `>= 2` defensores. Además aprende desde la 2.ª corrección consistente y muestra pista temprana en textos casi idénticos.
- **Identidad unificada de defensores:** Títulos como `Dr.`, `Dra.` o `Defensor Público` se unifican internamente para que variantes del mismo profesional compartan perfil e historial, y el renombrado desde el directorio traslada automáticamente reglas, alias, métricas y preferencias.
- **Defensor obligatorio al crear planillas:** Al crear nuevas planillas Excel ahora se exige asignar un Defensor Público; las planillas antiguas sin defensor siguen abriéndose con el perfil general y un aviso discreto para asignarlo en 1 clic.
- **Mapa de Conexiones local (`GrafoAsociaciones` v3):** Conecta localmente sin costo extra de tokens al defensor, sus siglas cortas de despacho (`J4`, `J10`), el prefijo municipal del radicado y el tipo de audiencia para desambiguar despachos en futuras extracciones.
- **Consolidador periódico con IA disparado estadísticamente:** En segundo plano y bajo tope de presupuesto (`<= 6%`), la IA fusiona reglas repetidas, detecta contradicciones, rescata patrones de correcciones sueltas, mantiene perfiles sintéticos cortos (`general` y por defensor) inyectables al extraer y genera sugerencias técnicas para futuras versiones (con interruptores y botones manuales en *Configurar IA → Diagnóstico técnico*).
- **Cuadro adaptativo "¿Por qué la corrección?":** Evalúa localmente (`0` tokens) cuándo preguntar según el valor de la corrección, muestra qué cambió (`IA → Usuario`), ofrece botones rápidos incluyendo `Dato externo (no estaba en el texto)` (sin contaminar reglas ni gastar tokens), se auto-oculta al cambiar de contexto y aparece con menor frecuencia a medida que el sistema aprende o si prefieres omitir.
- **Exportación JSON enriquecida y privada:** Al compartir datos se conservan los nombres reales de los defensores, sus métricas, el Mapa de Conexiones y las consolidaciones mientras procesados, radicados, cédulas y teléfonos permanecen estrictamente anonimizados.

## [4.1.1] - 2026-11-18
- Selector `AM`/`PM` alineado directamente junto a `HH:MM` en la captura de hora de audiencias.

## [4.1.0] - 2026-11-18
- Conteo histórico de uso para `Tipo de audiencia`, `Hora` y `Juzgado` (incluyendo elementos con `0` usos y opciones personalizadas), ampliado a `6` botones de acceso rápido de hora y exportación unificada en JSON.
- Portabilidad del historial de uso dentro de los paquetes completos `.lokus` y esquema JSON estricto nativo para OpenAI y Gemini con normalización sin tildes hacia el catálogo.

## [4.0.0] - 2026-11-18
- Sistema de autoaprendizaje estadístico bayesiano con gobernanza de reglas, detección de deriva, motor local en Rust puro, inducción agrupada por clústeres y panel de Diagnóstico Técnico.
