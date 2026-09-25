# Benchmarks reproducibles

[`scripts/benchmark.mjs`](../scripts/benchmark.mjs) mide el mismo programa Ostrin
por dos caminos:

1. `ostrinc --run`, que comprueba y ejecuta con el intérprete.
2. `ostrinc --compile`, seguido de la ejecución del binario nativo generado.

El workload [`examples/benchmark_numeric.ostrin`](../examples/benchmark_numeric.ostrin)
solo usa aritmética entera escalar y un rango determinista. Cada ejecución comprueba
que ambos caminos impriman exactamente el mismo resultado antes de guardar las
mediciones. El informe JSON incluye commit, plataforma, versión del compilador,
mediciones individuales, mediana, tiempo de compilación nativa y la razón entre
medianas.

El workflow [`benchmarks.yml`](../.github/workflows/benchmarks.yml) se ejecuta
manualmente o cada lunes y conserva el informe por commit durante 14 días. Las
mediciones son evidencia de una carga concreta; no representan por sí solas el
rendimiento de todo el lenguaje. La ampliación prevista cubre arrays, cantidades,
métodos numéricos y cargas científicas antes de establecer umbrales.

Para reproducirlo localmente:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --iterations 7 --warmups 1
```
