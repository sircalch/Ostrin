# Benchmarks reproducibles

[`scripts/benchmark.mjs`](../scripts/benchmark.mjs) mide el mismo programa Ostrin
por dos caminos:

1. `ostrinc --run`, que comprueba y ejecuta con el intérprete.
2. `ostrinc --compile`, seguido de la ejecución del binario nativo generado.

La batería predeterminada cubre [`benchmark_numeric.ostrin`](../examples/benchmark_numeric.ostrin)
(aritmética escalar), `arrays.ostrin`, `quantity_arrays.ostrin`, el par alineado
[`benchmark_float_array.ostrin`](../examples/benchmark_float_array.ostrin) /
[`benchmark_quantity_array.ostrin`](../examples/benchmark_quantity_array.ostrin),
`numeric_methods.ostrin` y cargas de álgebra lineal y visualización. El par
Float/Quantity conserva la misma forma y cantidad de operaciones (el control
Float multiplica explícitamente por `1.0`, donde Quantity multiplica por `1 m`)
para observar el coste semántico de las unidades; no es una comparación con
otros lenguajes. El runner comprueba además que sus valores numéricos coincidan
dentro de una tolerancia declarada. Cada ejecución comprueba que ambos caminos impriman
exactamente el mismo resultado antes de guardar las mediciones. El informe JSON
schema 3 incluye el hash exacto de cada fuente, commit y estado del checkout,
sistema operativo, CPU, compilador C, defaults administrados por el compilador e
inputs de flags, además de cada muestra y
estadísticas de dispersión (mínimo, máximo, media, mediana, p95 y desviación
estándar). El gate [`benchmark-contract-check.mjs`](../scripts/benchmark-contract-check.mjs)
rechaza informes incompletos o inconsistentes. La memoria pico del proceso hijo y
los contadores de asignaciones aún se declaran como no medidos.

La batería incluye también un control escalar de 50,000 sumas y una carga de
50,000 sumas de una `Measurement<Float>` con una fuente compartida. Esta segunda
carga comprueba que el coeficiente correlacionado se acumule correctamente y da
una referencia gruesa del costo en la misma máquina; incluye arranque de procesos,
envoltorios `Result`, ownership y gestión de mediciones, por lo que no aísla el
costo del campo de identidad.

El workflow [`benchmarks.yml`](../.github/workflows/benchmarks.yml) se ejecuta
manualmente o cada lunes y conserva el informe por commit durante 14 días. Las
mediciones son evidencia de una carga concreta; no representan por sí solas el
rendimiento de todo el lenguaje. La ampliación prevista cubre arrays, cantidades,
métodos numéricos y cargas científicas antes de establecer umbrales.

El historial local [`validation/benchmark-history.json`](../validation/benchmark-history.json)
conserva tres ejecuciones reales de commits anteriores. Cada entrada mantiene el
commit del benchmark, entorno, número de muestras, hashes de salida y medianas por
carga; los dos snapshots más recientes también conservan hashes de fuente. El script
[`scripts/benchmark-history.mjs`](../scripts/benchmark-history.mjs)
reconstruye ese índice desde los snapshots versionados; las entradas no se mezclan
entre máquinas ni se presentan como una comparación con otros lenguajes.
El índice también incluye una revisión automática de transiciones: solo compara
snapshots con el mismo entorno y el mismo hash de fuente, aplica un umbral de
regresión declarado del 25% y conserva como `NOT_COMPARABLE` el snapshot antiguo
que no tiene hashes de fuente. La transición comparable más reciente pasa con
regresiones máximas de 2.889% nativas y 2.126% interpretadas; no queda una
regresión comparable sin explicar.

Para reproducirlo localmente:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --iterations 7 --warmups 1
node scripts/benchmark-contract-check.mjs
```

Puedes seleccionar una fuente concreta o una lista separada por comas con
`--workloads examples/arrays.ostrin,examples/numeric_methods.ostrin`.
