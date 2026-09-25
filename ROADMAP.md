# Ostrin — roadmap

La fuente de verdad del roadmap activo es [ESTADO_Y_PLAN.md](ESTADO_Y_PLAN.md), sección 8 y sus
listas de pendientes. Este índice separa el futuro del estado comprobado.

## Siguiente ciclo

1. Reducir el fallback AST con el trinquete por archivo de `--native-type-report` (baseline actual:
   1.207 funciones IR, 584 HIR y 1.418 AST; el incremento acotado corresponde a las superficies
   `std.viz.boxplot` y `std.viz.table`, que quedan como deuda explícita para la siguiente migración).
2. Completar ownership sobre agregados, escapes, valores `Phi`, errores y formas anidadas, con
   leak-check y sanitizers como evidencia.
3. Añadir casos de compilación nativa y divergencia semántica al fuzzing de entradas válidas.
4. Medir cobertura reproducible y publicar benchmarks nativo frente a intérprete.
   La cobertura queda registrada por commit en `coverage.yml` como resumen y LCOV;
   `benchmarks.yml` ejecuta ahora cargas escalares, de arrays, cantidades y métodos
   numéricos, y conserva el JSON con las medianas de ambos caminos. Falta añadir
   cargas de álgebra lineal, visualización y comparaciones históricas antes de usarlo
   como presupuesto de rendimiento.
5. **Objetivo de distribución y reconocimiento en GitHub Linguist**: reunir uso público
   distribuido y licencias trazables; preparar la definición de lenguaje (`languages.yml`,
   extensiones, gramática y muestras); mantener el borrador en `docs/linguist.md`; validarla
   contra `github-linguist`; y abrir la propuesta upstream. Después de su aceptación y de una
   versión publicada de Linguist, verificar la clasificación de `.ostrin` en GitHub y
   documentar el resultado en la release y el sitio.
6. **Continuar el frente profesional de visualización y web**: ampliar `std.viz` con
   gráficas estadísticas, tablas interactivas y selección enlazada, mantener ejemplos
   reproducibles en la galería, consolidar la exportación WebM y añadir exportación
   MP4/GIF/PNG/PDF, exploración 3D, WebGPU y procedencia de figuras con evidencia en
   intérprete, nativo y WASM.

## Horizonte posterior

- Unidades afines y prefijos automáticos.
- Autodiff inverso, más álgebra lineal y métodos numéricos.
- Exportación PNG/PDF/HTML y controles temporales de animación.
- FFI C, registry público y canales de distribución adicionales.
- Reconocimiento de Ostrin en GitHub Linguist y aparición de `.ostrin` en el mapa de lenguajes.
- GPU/WebGPU después de estabilizar Array, IR y ownership.

Las capacidades implementadas no se anuncian aquí hasta que tengan evidencia en el estado actual,
las pruebas o los workflows correspondientes.
