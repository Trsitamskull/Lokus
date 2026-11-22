# Historial de Versiones (Lokus)

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
