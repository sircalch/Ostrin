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
- bundles R3 con inventario de efectos parcial y total de llamadas sin clasificar;
- estado de cada criterio de readiness de v0.2;
- presencia de validación numérica, auditoría web, Linguist y auditoría de ramas.

Antes de la verificación web, `scripts/fallback-ledger-check.mjs` comprueba que el
ledger conserve su esquema, baseline y campos de evidencia.

El repositorio conserva además el último resultado local de Windows en
[`validation/numeric/windows-local.json`](../validation/numeric/windows-local.json).
Ese archivo documenta una ejecución reproducible local; no sustituye el enlace
de CI exigido por el gate de release.
El estado local sólo es `verified` si los hashes SHA-256 del contrato y el fixture
coinciden con los archivos actuales, el binario utilizado tiene un hash registrado,
los seis informes LU/Cholesky/QR/SVD/autovalores/FFT están completos en ambos
backends, los residuales quedan dentro de las tolerancias declaradas y los conteos
de 80 valores resumidos y 136 emitidos coinciden con el contrato. Reducir el conteo
de paridad dentro del propio reporte no puede convertir un informe truncado en verde.
El hash del binario identifica la ejecución local; no acredita por sí solo qué
commit lo compiló.
Los hashes de los dos archivos de texto se calculan con saltos de línea LF
normalizados para que el mismo contenido coincida en Windows, Linux y macOS;
el hash del compilador corresponde a sus bytes originales.

La atribución `ci=verified` exige que el commit y la URL de GitHub Actions del
reporte coincidan con la revisión actual que ejecuta el scorecard. La revisión
fuente del candidato v0.2 permanece separada en `sourceRevision`; no se usa para
atribuir validaciones numéricas posteriores. Un reporte local sin procedencia CI
permanece `unattributed` aunque pase la validación numérica.

El archivo incluye el commit fuente para que un informe histórico pueda
reproducirse. No presenta como verdes los criterios que siguen abiertos en el
manifiesto de release.

`workingTreeDirty` detecta cambios de fuente y otros archivos locales, pero ignora
los bundles de experimento, el reporte R3, el scorecard y los datos de estado que
la verificación de Pages regenera como evidencia de host. Así, esos artefactos CI
no convierten por sí mismos un checkout limpio en una fuente marcada como sucia.
