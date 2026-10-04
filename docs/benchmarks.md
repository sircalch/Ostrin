# Benchmarks reproducibles

[`scripts/benchmark.mjs`](../scripts/benchmark.mjs) mide el mismo programa Ostrin
por dos caminos:

1. `ostrinc --run`, que comprueba y ejecuta con el intérprete.
2. `ostrinc --compile`, seguido de la ejecución del binario nativo generado.

La batería predeterminada cubre [`benchmark_numeric.ostrin`](../examples/benchmark_numeric.ostrin)
(aritmética escalar), `arrays.ostrin`, `quantity_arrays.ostrin` y
`numeric_methods.ostrin`. Cada ejecución comprueba que ambos caminos impriman
exactamente el mismo resultado antes de guardar las mediciones. El informe JSON
schema 3 incluye el hash exacto de cada fuente, commit y estado del checkout,
sistema operativo, CPU, compilador C, defaults administrados por el compilador e
inputs de flags, además de cada muestra y
estadísticas de dispersión (mínimo, máximo, media, mediana, p95 y desviación
estándar). El gate [`benchmark-contract-check.mjs`](../scripts/benchmark-contract-check.mjs)
rechaza informes incompletos o inconsistentes. La memoria pico del proceso hijo y
los contadores de asignaciones aún se declaran como no medidos.

El workflow [`benchmarks.yml`](../.github/workflows/benchmarks.yml) se ejecuta
manualmente o cada lunes y conserva el informe por commit durante 14 días. Las
mediciones son evidencia de una carga concreta; no representan por sí solas el
rendimiento de todo el lenguaje. La ampliación prevista cubre arrays, cantidades,
métodos numéricos y cargas científicas antes de establecer umbrales.

Para reproducirlo localmente:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --iterations 7 --warmups 1
node scripts/benchmark-contract-check.mjs
```

Puedes seleccionar una fuente concreta o una lista separada por comas con
`--workloads examples/arrays.ostrin,examples/numeric_methods.ostrin`.
