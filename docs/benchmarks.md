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
schema 3 incluye el hash exacto de cada fuente, compilador Ostrin y ejecutable
nativo, commit y estado del checkout, sistema operativo, CPU, compilador C,
defaults administrados por el compilador e inputs de flags, además de cada muestra y
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

La batería también incluye `viz_pendulum_calibration.ostrin`, el flujo científico
M1. El runner copia su snapshot versionado a un directorio aislado dentro de
`target/benchmarks/runtime-inputs`, registra ruta, tamaño y SHA-256 del snapshot,
y comprueba que el hash copiado sea idéntico antes de medir. El intérprete y el
binario nativo se ejecutan desde ese mismo directorio y sus salidas deben coincidir
exactamente. Las duraciones interpretadas y nativas son de extremo a extremo:
incluyen arranque del proceso, lectura del JSON, análisis, propagación de
incertidumbre y renderizado SVG; no aíslan el tiempo del núcleo numérico. La
compilación se mide aparte. `--compile-iterations N` repite esa medición y el
informe conserva muestras, mediana y dispersión; por defecto se hace una sola
compilación para limitar el costo de la batería completa.

Para repetir la medición enfocada del flujo M1 con siete muestras de ejecución y
cinco muestras de compilación:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --workloads examples/viz_pendulum_calibration.ostrin --iterations 7 --warmups 1 --compile-iterations 5
node scripts/benchmark-contract-check.mjs
```

El workflow [`benchmarks.yml`](../.github/workflows/benchmarks.yml) ejecuta el
flujo file-backed del péndulo en Ubuntu, macOS y Windows cuando se solicita
manualmente, cada lunes o cuando cambia el compilador, el runner o sus datos en
un PR o en `main`. Cada plataforma conserva un artefacto separado por commit
durante 30 días, con siete ejecuciones del intérprete, siete nativas y cinco
compilaciones. Los runners compartidos varían entre ejecuciones: estos informes
son mediciones con procedencia por host, no umbrales de regresión ni evidencia
de velocidad general o equivalencia temporal entre sistemas.

El historial local [`validation/benchmark-history.json`](../validation/benchmark-history.json)
conserva ejecuciones reales de snapshots versionados. Cada entrada mantiene el
commit del benchmark, entorno, número de muestras, hashes de salida y medianas por
carga; los snapshots recientes también conservan hashes de fuente. El script
[`scripts/benchmark-history.mjs`](../scripts/benchmark-history.mjs)
reconstruye ese índice desde los snapshots versionados; las entradas no se mezclan
entre máquinas ni se presentan como una comparación con otros lenguajes.
El índice también incluye una revisión automática de transiciones: solo compara
snapshots con el mismo entorno y el mismo hash de fuente, aplica un umbral de
regresión declarado del 25% y conserva como `NOT_COMPARABLE` el snapshot antiguo
que no tiene hashes de fuente. La última transición comparable (310ed6c → b3c997e)
pasó con regresiones máximas de 2.889% nativas y 2.126% interpretadas. El snapshot
`8263d01` registra el nuevo benchmark en Windows x64, pero queda `NOT_COMPARABLE`
con el anterior porque cambió la versión del compilador de 0.1.0 a 0.2.0 y cambió
la fuente del workload Float; no se interpreta como regresión ni mejora.

La corrida local más reciente está registrada en
[`validation/benchmark-m1-covariance-windows-5998717.json`](../validation/benchmark-m1-covariance-windows-5998717.json), SHA-256
`189fd80377da23adbce55173324045237625891df4aee618dc0137f0173f98cc`: source
`59987174444b03df5c1fd310bacc5386436b232b`, Windows 11 x64, 15 workloads,
7 muestras y 1 calentamiento, con intérprete y binario nativo verificando salida
idéntica. Para `measurement_covariance.ostrin`, la mediana fue 84.802 ms en el
intérprete y 8.272 ms en nativo (10.252× en esta carga concreta). El informe
registra hashes de fuente y salida; no mide RSS/memoria pico y no permite afirmar
rendimiento entre plataformas o frente a otros lenguajes.

Para reproducirlo localmente:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --iterations 7 --warmups 1
node scripts/benchmark-contract-check.mjs
```

Puedes seleccionar una fuente concreta o una lista separada por comas con
`--workloads examples/arrays.ostrin,examples/numeric_methods.ostrin`.
