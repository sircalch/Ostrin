# Cobertura reproducible del compilador

El workflow [`coverage.yml`](../.github/workflows/coverage.yml) ejecuta la suite
completa del compilador con `cargo-llvm-cov` y deja dos salidas en el artefacto
de la ejecución:

- `summary.txt`: resumen legible que también aparece en el resumen de GitHub Actions.
- `lcov.info`: cobertura LCOV para abrirla en herramientas compatibles o procesarla
  en una comprobación posterior.

La medición usa Rust `1.98.1`, `llvm-tools-preview` y `cargo-llvm-cov 0.9.1`.
Se activa manualmente o cada lunes por la programación semanal. Los artefactos se
conservan 14 días para que cada medición tenga un identificador de commit y no se
confunda con una cifra generada por otra revisión.

Para reproducirla localmente con la misma herramienta, instala el componente LLVM
y ejecuta:

```text
cargo install cargo-llvm-cov --version 0.9.1 --locked
cargo llvm-cov --manifest-path compiler/Cargo.toml --all-targets --summary-only
```

La cobertura queda registrada como evidencia del estado del código; todavía no se
impone un umbral mínimo porque el backend nativo y las rutas de fallback están en
migración. El siguiente paso es comparar esta salida con benchmarks repetibles del
intérprete y del backend nativo.
