# Historial de Versiones (Lokus)

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
