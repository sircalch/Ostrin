# Ostrin — roadmap

La fuente de verdad del roadmap activo es [ESTADO_Y_PLAN.md](ESTADO_Y_PLAN.md), sección 8 y sus
listas de pendientes. Este índice separa el futuro del estado comprobado.

## Siguiente ciclo

1. Reducir el fallback AST con el trinquete por archivo de `--native-type-report` (baseline actual:
   6.501 funciones generadas por HIR/IR (6.488 IR + 13 HIR) y 50 AST). Los patrones superficiales de records con campos escalares
   y los enums no genéricos con payloads escalares, incluidos los enteros de ancho fijo, variantes unitarias, patrones simples y
   patrones anidados de enums por valor y sus literales/rangos escalares ya comparten IR/C; quedan pendientes los payloads
   gestionados y enums genéricos.
   Los métodos de records y enums concretos ya comparten
   la ruta IR/C cuando sus campos y ownership son compatibles; los records recursivos permanecen
   deliberadamente en HIR/AST hasta cerrar ese contrato. El puente IR para los constructores numéricos
   `zeros`/`ones`, `norm`, `abs` escalar y `sqrt` migró 348 funciones compartidas por la suite;
   LU ya está disponible con paridad publicada en el Lab; QR y Cholesky cruzan la IR y conservan esa
   paridad. El renderer de `std.viz` y sus superficies 2D/3D ya cruzan IR/C; las
   instanciaciones genéricas de trazado con `Quantity` ya comparten esa ruta; quedan funciones
   numéricas auxiliares y el ejemplo compuesto de selección enlazada, cuyos agregados y ownership siguen necesitando la migración gradual. La preparación numérica
   de histogramas y violines ya cruza IR/C mediante `histogram`, `linspace`, `pow`, `norm_pdf` y
   `norm_cdf`, y los ids
   deterministas de `std.viz::uid` usan `hash(String)` en IR/C y el renderer usa un recorrido IR
   explícito sobre sus series.
   La selección numérica `a[mask]`, `where(mask, a, b)`, `not` sobre `Array<Bool>` y los
   cortes `to`/`until` y la negación unaria elemento a elemento ya usan los kernels IR/C para `Int`, `Float`, `Float32` y `Bool`, con
   temporales escalares de `where` liberados dentro del C generado; la selección enmascarada
   de `Array<Quantity<D>>` conserva su unidad mediante el mismo camino IR/C. `where` y otras
   formas complejas sobre `Quantity` siguen fuera de esta pasada. La promoción comprobada de un
   escalar `Float` a las operaciones de `Array<Float32>` también usa el kernel escalar IR/C,
   con paridad de `arrays.ostrin` y su fixture dedicado.
   El builtin `format` para `String` y `List<String>` ya cruza IR/C y `std.strings::format_text`
   dejó el fallback AST. `std.numeric.Complex` ya tiene una API experimental de `Float64` y una demo live; sus métodos de
   trait con `Self` se sustituyen en HIR y ya bajan a IR/C; `ComplexVector`
   y `ComplexMatrix` añaden almacenamiento denso dividido, adjunta, multiplicación y `solve` con
   pivoteo parcial, con una demo live en Linear Algebra. El tipo paramétrico, `Array<Complex>`,
   matrices dispersas y descomposiciones complejas siguen pendientes. `std.numeric.lu`
   ahora cubre eliminación con pivoteo parcial, reconstrucción, determinante y resolución triangular;
   `std.numeric.eigen` añade autovalores y autovectores ortonormales para matrices simétricas;
   `std.numeric.svd` añade SVD fina densa, rango, número de condición y resolución por pseudoinversa. Sus métodos con
   las cifras históricas de deuda de LU, SVD y autovectores quedaron superadas por la migración
   de métodos; los recorridos matriciales y agregados que aún requieren ownership anidado siguen
   pendientes del siguiente pase. La primera
   migración de arrays gestionados ya está en la IR/C: `Array<Quantity<D>>` conserva la unidad
   al indexar y al construir arrays desde listas, y `std.numeric` usa ese camino en sus
   integrales e interpolación con cantidades. Esta pasada añade aritmética elemento a elemento,
   comparaciones, reducciones, negación, cortes por rango, conversiones y `to_list` de arrays
   con unidades, con helpers IR que preservan la etiqueta; quedan cambios de forma complejos y
   agregados anidados para ciclos posteriores. `List<Array<Quantity<D>>>` ya reutiliza el backing
   `Array_Float`, conserva la etiqueta de unidad y libera cada array desde el destructor de la lista,
   con paridad intérprete/nativo/WASI y `live_allocations=0` en la prueba dedicada.
   La representación de arrays consume ahora sus temporales de texto; `quantity_arrays.ostrin`
   registra `live_allocations=4` bajo leak-check nativo. `Array<Quantity<D>>.to_list()` retiene
   ahora las etiquetas dinámicas por elemento y las libera con el destructor de la lista; las
   expresiones escalares y las etiquetas que escapan a valores escalares siguen como deuda de
   ownership para un pase posterior.
   `Rng` y sus métodos escalares y de muestreo (`rand`, `randn`, `randint`, `permutation`) ya
   comparten IR/C con ownership explícito y paridad intérprete/nativo. Los combinadores de listas
   (`map`, `filter`, `fold`, `any`, `all`, `find`) también cruzan IR/C, incluidos cierres con
   capturas y resultados gestionados; la siguiente familia es ownership sobre agregados anidados
   e iteradores.
   Los iteradores genéricos sustituyen ahora su elemento concreto también cuando `T` es un
   agregado gestionado anidado, manteniendo la ruta IR/C y el ownership de la lista devuelta.
   Las conversiones escalares `as<Float/Float32>` y las conversiones comprobadas a `Int`/enteros de ancho fijo,
   `abs` sobre arrays y las funciones matemáticas
   deterministas (`sin`, `cos`, `ln`, `exp`, `pi`, `eye` y relacionadas) ya cruzan IR/C; esta pasada
   redujo el fallback AST medido sin cambiar la salida del intérprete ni del nativo. `Int.to_string`,
   `Float.to_string`, `Float32.to_string` y enteros de ancho fijo ya están disponibles en IR/C, con
   `std.numeric.secant`, `std.numeric.newton` y `std.numeric.powi` como consumidores verificados;
   los operadores aritméticos definidos por records ahora pueden despacharse desde IR cuando ambos
   operandos tienen el mismo record concreto. Los métodos de texto `String.char_at`, `String.slice`
   y `String.codepoint` también cruzan IR/C, con liberación lineal del receptor y paridad nativo/
   intérprete comprobada. `List<String>.join` también usa la ruta IR/C y conserva ownership
   lineal al construir la cadena unida. `parse_csv` construye ahora `List<List<String>>` desde
   IR/C, libera las filas temporales y conserva paridad en nativo y WASI.
   El flujo `dataframe.ostrin` también baja sus ramas `panic` y `corr` de arrays desde IR/C,
   dejando sus recorridos de columnas anidadas sin fallback AST y con leak-check.
   El lowering de ownership aplica ahora un contrato uniforme de llamada prestada a los métodos:
   los constructores encadenados de `Figure`, `Table` y `Scene3D` liberan sus aliases locales
   después de cada llamada y ya no fuerzan el `main` completo al AST; `viz_hexbin` conserva
   paridad intérprete/nativo y una aserción de `ir=1, ast=0`.
2. Completar ownership sobre agregados, escapes, valores `Phi`, errores y formas anidadas, con
   leak-check y sanitizers como evidencia.
3. Añadir casos de compilación nativa y divergencia semántica al fuzzing de entradas válidas.
4. Medir cobertura reproducible y publicar benchmarks nativo frente a intérprete.
   La cobertura queda registrada por commit en `coverage.yml` como resumen y LCOV;
   `benchmarks.yml` ejecuta ahora cargas escalares, de arrays, cantidades, métodos
   numéricos, álgebra lineal densa, álgebra compleja y visualización SVG, y conserva
   el JSON con las medianas de ambos caminos. `website/benchmarks.html` publica un
   registro reproducible con commit, entorno, muestras, hashes de salida y método;
   faltan comparaciones históricas y mediciones externas antes de usarlo como presupuesto
   de rendimiento entre versiones o lenguajes.
5. **Objetivo de distribución y reconocimiento en GitHub Linguist**: reunir uso público
   distribuido y licencias trazables; preparar la definición de lenguaje (`languages.yml`,
   extensiones, gramática y muestras); mantener el borrador en `docs/linguist.md`; validarla
   contra `github-linguist`; conservar el snapshot reproducible de uso en
   [`docs/linguist-usage.md`](docs/linguist-usage.md); y abrir la propuesta upstream solo
   cuando se cumplan los criterios de uso. Después de su aceptación y de una versión publicada
   de Linguist, verificar la clasificación de `.ostrin` en GitHub y documentar el resultado en
   la release y el sitio. Antes de esa propuesta, el frente de adopción debe publicar la
   extensión de VS Code, instaladores y plantillas de proyectos; definir el registry de paquetes;
   reunir casos de estudio y proyectos externos verificables; y mantener una página de releases,
   búsqueda de documentación y benchmarks históricos sin inflar métricas con forks.
6. **Continuar el frente profesional de visualización y web**: ampliar `std.viz` con
   campos vectoriales 2D (`quiver` y `streamplot`), gráficas estadísticas (incluidos violines KDE y hexbin), bandas de contorno rellenas y consolidar la selección enlazada, mantener tablas reproducibles con filtro y
   ordenamiento en el explorador, conservar ejemplos verificables en la galería, ampliar animaciones,
   vídeo, controles temporales, calidad 1×/2×, interruptores de series desde la leyenda, crosshair de inspección y parámetros conducidos por Ostrin, consolidar la exportación SVG/PNG/HTML/WebM/GIF, mantener el flujo PDF vectorial y ofrecer MP4 cuando el navegador exponga un códec compatible, exploración 3D (ya incluye superficies,
   campos vectoriales, cámara, cortes ortogonales e isosuperficies de volúmenes), WebGPU y ampliar la procedencia reproducible ya disponible
   (hashes de fuente/datos, semilla y compilador) a informes y flujos de publicación con
   evidencia en intérprete, nativo y WASM.
   El primer artefacto web de alto retorno ya está implementado como un registro de dos fixtures
   source-backed (`provenance` y la superficie 3D `surface`): `ostrin.experiment/v0` descarga código,
   entradas declaradas, SVG, manifest y `provenance.json` con hashes calculados y etiqueta R0; R1
   queda como replay verificable planificado y no se reclaman R2/R3. El siguiente paso es extender
   el contrato al resto de experimentos de la galería, incluyendo parámetros y cámara seleccionados,
   para que la galería funcione como superficie de publicación científica además de demo.
   El Scientific Lab ya conserva la demo activa y sus parámetros en la URL para compartir y
   restaurar experimentos reproducibles desde el navegador. La navegación compacta y el catálogo
   de ejemplos ya tienen estados ARIA, foco de teclado, Escape, pestañas con flechas/Home/End y
   filtros anunciados; Chromium cubre los flujos completos y Firefox/WebKit ya recorren las catorce
   páginas públicas con una matriz de humo; la galería añade ahora tres recorridos source-backed
   (`simulate`, `analyze` y `explore-3d`) que agrupan figuras, tablas, animaciones y procedencia
   mediante `?workflow=...`, con etiquetas `available`/`experimental`; quedan auditorías más amplias
   con lectores de pantalla y baselines visuales.
7. **Extender el núcleo científico de mediciones**: validar el API escalar experimental de
   `std.measurements`; la proyección de series `List<Measurement<Float>>` a valores e
   incertidumbres ya alimenta `std.viz.errorbars` con paridad intérprete/native/WASM. El siguiente
   bloque es integrar `Quantity<T>`, `Array<Measurement<T>>` y covarianza antes de añadir Monte
   Carlo, intervalos de cobertura o semántica de operadores.
8. **Diseñar efectos científicos y procedencia**: convertir la propuesta de [documento 26](docs/design/26-efectos-cientificos.md)
   en un sistema de efectos interno con capacidades explícitas, niveles R0–R3 y un artefacto JSON
   reproducible antes de fijar sintaxis pública. El inventario experimental ya está disponible con
   `ostrinc --effect-report` y `--provenance-report`; el artefacto se publica en
   [`website/provenance.html`](website/provenance.html) con hash de fuente, target y límites de replay.
   Sigue siendo evidencia conservadora, no checking estático ni una garantía R2/R3.

## Horizonte posterior

- Integración completa de Measurement<T> e incertidumbre en [docs/design/25-mediciones-e-incertidumbre.md](docs/design/25-mediciones-e-incertidumbre.md), junto con efectos científicos y procedencia en [docs/design/26-efectos-cientificos.md](docs/design/26-efectos-cientificos.md); la fase escalar y la primera proyección de series a error bars ya tienen ejemplos, paridad y figura de galería. Quantity, `Array<Measurement<T>>`, covarianza, Monte Carlo y cobertura siguen condicionados a revisión de modelo, paridad y pruebas.
- Unidades afines y prefijos automáticos.
- Autodiff inverso, más álgebra lineal y métodos numéricos.
- Mayor calidad de vídeo y codecs adicionales; los contratos de eventos ricos ya están disponibles con `.bind(channel, event)` para enlazar marcas indexadas entre figuras, y la exportación HTML autocontenida, los controles numéricos declarativos conducidos por Ostrin, SVG/PNG/WebM/GIF, MP4 cuando existe el codec y el flujo PDF vectorial con metadata de procedencia ya tienen ruta web.
- FFI C, registry público y canales de distribución adicionales.
- Reconocimiento de Ostrin en GitHub Linguist y aparición de `.ostrin` en el mapa de lenguajes.
- GPU/WebGPU después de estabilizar Array, IR y ownership.

Las capacidades implementadas no se anuncian aquí hasta que tengan evidencia en el estado actual,
las pruebas o los workflows correspondientes.
