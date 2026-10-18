# Reproducir evidencia de Ostrin

Este documento describe cómo reproducir los artefactos científicos que el repositorio
publica. Los comandos se ejecutan desde la raíz y no requieren servicios externos para
la validación local.

## Preparar el compilador

```text
cargo build --manifest-path compiler/Cargo.toml --release
```

## Validar el backend numérico

La suite usa casos analíticos independientes para LU, Cholesky, QR, SVD y autovalores
simétricos. Compara intérprete y ejecutable nativo, comprueba residuos con tolerancias
por caso y exige `live_allocations=0`.

```text
node scripts/numeric-validation.mjs
node --test scripts/numeric-validation.test.mjs
```

En GitHub Actions el reporte JSON añade la URL exacta de la ejecución. Un reporte local
no se presenta como evidencia CI.

## Reproducir figuras

Los 29 bundles source-backed contienen `source.ostrin`, `data.json`, `figure.svg` y
`provenance.json` con hashes. El replay ejecutable descubre los cortes R1/R2 desde el
registro, verifica igualdad del SVG y comprueba sensibilidad cuando el contrato la declara.

```text
node scripts/experiment-replay.mjs --all
node --test scripts/experiment-replay.test.mjs
node scripts/experiment-bundle.mjs
node --test scripts/experiment-bundle.test.mjs
```

## Niveles de reproducibilidad

- **R0:** fuente, entrada y salida registradas; el replay exacto está limitado a la configuración de CI declarada.
- **R1:** R0 más semilla, algoritmo y sensibilidad de azar verificadas.
- **R2:** R0/R1 más snapshot de entrada embebido, capacidades declaradas y sensibilidad de parámetros verificada.
- **R3:** artefacto auditable con host, dependencias, scheduler, efectos y evidencia ejecutable. La CI genera 29/29 capturas estructurales en su host; ese conteo no promociona los bundles a R3. El primer corte adicional es un recibo de replay WASI para `surface`, ligado al run, al binario ejecutado y a los hashes del bundle. El scorecard conserva R3=0.

```text
node scripts/r3-readiness.mjs --write
```

En Website CI, después del replay general de las figuras, `surface` genera un recibo efímero
y el verificador exige que pertenezca al mismo `tested_sha`/run y a los mismos bytes:

```text
node scripts/experiment-replay.mjs --fixture surface --write-r3-receipt
node scripts/r3-surface-receipt.mjs --verify
```

Estos comandos requieren el bundle host-bound y las variables de la ejecución de GitHub Actions;
un checkout local ordinario no debe inventarlas.
El artefacto del run también conserva `website/ostrinc.wasm`, los bundles y el recibo. Tras
descargarlo, se puede verificar sin reconstruir el binario local (sustituye los valores por los
del run y su `tested_sha` publicados en GitHub Actions):

```text
node scripts/r3-surface-receipt.mjs --verify-artifact <directorio-descargado> --run-id <id> --tested-sha <sha>
```

Pages CI usa `OSTRIN_CAPTURE_HOST=1` y exige:

```text
node scripts/r3-readiness.mjs --write --require-ready
```

La captura de host incluye solo etiquetas públicas del runner, arquitectura, imagen,
Node, WASI y hardware visible del proceso. No incluye hostname, usuario ni rutas privadas.

## Comprobaciones de integridad

```text
node scripts/fallback-ledger-check.mjs
node scripts/website-check.mjs
node scripts/project-plan-check.mjs
```

Las tolerancias numéricas son específicas de los casos declarados, los resultados entre
plataformas no se comparan bit a bit y los bundles R0/R1/R2 no se promocionan manualmente a R3.
