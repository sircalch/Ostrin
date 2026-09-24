# Ostrin — roadmap

La fuente de verdad del roadmap activo es [ESTADO_Y_PLAN.md](ESTADO_Y_PLAN.md), sección 8 y sus
listas de pendientes. Este índice separa el futuro del estado comprobado.

## Siguiente ciclo

1. Reducir el fallback AST con el trinquete por archivo de `--native-type-report` (baseline actual:
   913 funciones IR, 457 HIR y 1.432 AST).
2. Completar ownership sobre agregados, escapes, valores `Phi`, errores y formas anidadas, con
   leak-check y sanitizers como evidencia.
3. Añadir casos de compilación nativa y divergencia semántica al fuzzing de entradas válidas.
4. Medir cobertura reproducible y publicar benchmarks nativo frente a intérprete.

## Horizonte posterior

- Unidades afines y prefijos automáticos.
- Autodiff inverso, más álgebra lineal y métodos numéricos.
- Exportación PNG/PDF/HTML y controles temporales de animación.
- FFI C, registry público y canales de distribución adicionales.
- GPU/WebGPU después de estabilizar Array, IR y ownership.

Las capacidades implementadas no se anuncian aquí hasta que tengan evidencia en el estado actual,
las pruebas o los workflows correspondientes.
