# Quality scorecard

El scorecard reúne indicadores verificables del repositorio sin convertir la
calidad en una puntuación única. Cada bloque conserva su fuente y su estado.

Generarlo y actualizar el artefacto local:

```text
node scripts/quality-scorecard.mjs --write
```

El resultado se guarda en
[`validation/quality-scorecard.json`](../validation/quality-scorecard.json) y
publica:

- conteo de pruebas Rust marcadas con `#[test]`;
- funciones IR/HIR y límite de fallback AST;
- ledger machine-readable de cada fallback AST restante en
  [`validation/ast-fallback-ledger.json`](../validation/ast-fallback-ledger.json);
- celdas `TESTED`, `SUPPORTED` y `KNOWN LIMITATION` de la matriz de ownership;
- distribución R0/R1/R2/R3 del registro de experimentos;
- estado de auditoría R3 para los 29 bundles publicados;
- estado de cada criterio de readiness de v0.2;
- presencia de validación numérica, auditoría web, Linguist y auditoría de ramas.

Antes de la verificación web, `scripts/fallback-ledger-check.mjs` comprueba que el
ledger conserve su esquema, baseline y campos de evidencia.

El repositorio conserva además el último resultado local de Windows en
[`validation/numeric/windows-local.json`](../validation/numeric/windows-local.json).
Ese archivo documenta una ejecución reproducible local; no sustituye el enlace
de CI exigido por el gate de release.

El archivo incluye el commit fuente para que un informe histórico pueda
reproducirse. No presenta como verdes los criterios que siguen abiertos en el
manifiesto de release.
