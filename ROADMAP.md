# Ostrin — roadmap

La fuente de verdad del roadmap activo es [ESTADO_Y_PLAN.md](ESTADO_Y_PLAN.md), sección 8 y sus
listas de pendientes. Este índice separa el futuro del estado comprobado.

## Siguiente ciclo

1. Reducir el fallback AST con el trinquete por archivo de `--native-type-report` (baseline actual:
   6.248 funciones IR, 125 HIR y 131 AST). Los métodos de records y enums concretos ya comparten
   la ruta IR/C cuando sus campos y ownership son compatibles; los records recursivos permanecen
   deliberadamente en HIR/AST hasta cerrar ese contrato. El puente IR para los constructores numéricos
   `zeros`/`ones`, `norm`, `abs` escalar y `sqrt` migró 348 funciones compartidas por la suite;
   LU ya está disponible con paridad publicada en el Lab; QR y Cholesky cruzan la IR y conservan esa
   paridad. El renderer de `std.viz` y sus superficies 2D/3D ya cruzan IR/C; quedan dos
   instanciaciones genéricas de trazado con `Quantity`, funciones numéricas auxiliares y el ejemplo
   compuesto de selección enlazada, cuyos agregados y ownership siguen necesitando la migración gradual. La preparación numérica
   de histogramas y violines ya cruza IR/C mediante `histogram`, `linspace` y `pow`, y los ids
   deterministas de `std.viz::uid` usan `hash(String)` en IR/C y el renderer usa un recorrido IR
   explícito sobre sus series.
   `std.numeric.Complex` ya tiene una API experimental de `Float64` y una demo live; sus métodos de
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
   integrales e interpolación con cantidades; la aritmética elemento a elemento de arrays con
   unidades sigue deliberadamente en HIR/AST hasta tener helpers IR que preserven la etiqueta.
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
   lineal al construir la cadena unida.
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
   contra `github-linguist`; y abrir la propuesta upstream. Después de su aceptación y de una
   versión publicada de Linguist, verificar la clasificación de `.ostrin` en GitHub y
   documentar el resultado en la release y el sitio.
6. **Continuar el frente profesional de visualización y web**: ampliar `std.viz` con
   campos vectoriales 2D (`quiver` y `streamplot`), gráficas estadísticas (incluidos violines KDE y hexbin), bandas de contorno rellenas y consolidar la selección enlazada, mantener tablas reproducibles con filtro y
   ordenamiento en el explorador, conservar ejemplos verificables en la galería, ampliar animaciones,
   vídeo, controles temporales, calidad 1×/2×, interruptores de series desde la leyenda, crosshair de inspección y parámetros conducidos por Ostrin, consolidar la exportación SVG/PNG/HTML/WebM/GIF, mantener el flujo PDF vectorial y ofrecer MP4 cuando el navegador exponga un códec compatible, exploración 3D (ya incluye superficies,
   campos vectoriales, cámara, cortes ortogonales e isosuperficies de volúmenes), WebGPU y ampliar la procedencia reproducible ya disponible
   (hashes de fuente/datos, semilla y compilador) a informes y flujos de publicación con
   evidencia en intérprete, nativo y WASM.
   El Scientific Lab ya conserva la demo activa y sus parámetros en la URL para compartir y
   restaurar experimentos reproducibles desde el navegador.

## Horizonte posterior

- Unidades afines y prefijos automáticos.
- Autodiff inverso, más álgebra lineal y métodos numéricos.
- Mayor calidad de vídeo y codecs adicionales; los contratos de eventos ricos ya están disponibles con `.bind(channel, event)` para enlazar marcas indexadas entre figuras, y la exportación HTML autocontenida, los controles numéricos declarativos conducidos por Ostrin, SVG/PNG/WebM/GIF, MP4 cuando existe el codec y el flujo PDF vectorial con metadata de procedencia ya tienen ruta web.
- FFI C, registry público y canales de distribución adicionales.
- Reconocimiento de Ostrin en GitHub Linguist y aparición de `.ostrin` en el mapa de lenguajes.
- GPU/WebGPU después de estabilizar Array, IR y ownership.

Las capacidades implementadas no se anuncian aquí hasta que tengan evidencia en el estado actual,
las pruebas o los workflows correspondientes.
