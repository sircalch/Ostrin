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
- **R3:** artefacto auditable con host, dependencias, scheduler, efectos y evidencia ejecutable. La CI genera 29/29 capturas estructurales en su host; ese conteo no promociona los bundles a R3. El run de `main` [`bd021d2` / 39700812531](https://github.com/sircalch/Ostrin/actions/runs/39700812531) produjo el artefacto host-bound [12393350506](https://github.com/sircalch/Ostrin/actions/runs/39700812531/artifacts/12393350506). Una reproducción independiente de los bytes descargados ejecutó los 29 bundles con resultado `match` y confirmó las evidencias archivadas de lectura y azar. Esto acredita el WASM y host de ese run; no completa el contrato R3 ni establece equivalencia entre plataformas. El scorecard de ese artefacto conserva R3=0.

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
descargarlo, se puede verificar y **volver a ejecutar** `surface` sin reconstruir el binario
local (sustituye los valores por los del run y su `tested_sha` registrados en Actions):

```text
node scripts/r3-surface-receipt.mjs --verify-artifact <directorio-descargado> --run-id <id> --tested-sha <sha>
node scripts/experiment-replay.mjs --replay-artifact <directorio-descargado> --run-id <id> --tested-sha <sha>
node scripts/experiment-replay.mjs --replay-artifact-all <directorio-descargado> --run-id <id> --tested-sha <sha>
```

El primer comando sólo verifica estructura, revisiones y hashes. El segundo ejecuta el WASM
archivado en procesos Node WASI nuevos, compara el SVG completo con el bundle, exige que el
cambio de parámetro altere la figura y contrasta el hash del módulo cargado con el recibo.
`--replay-artifact-all` aplica el replay a los **29 bundles R0/R1/R2** del inventario archivado
(18 R0 y 11 R1/R2); exige que cada captura pertenezca al run/SHA indicado y que todas las
ejecuciones carguen el módulo identificado por el recibo de `surface`. Los R0 se vuelven a ejecutar
dos veces y se comparan sus salidas; los R1/R2 validan su evidencia de sensibilidad y semilla.
El resultado describe cada replay en ese host; no establece equivalencia general entre plataformas.
El replay ejecutable no promueve por sí solo ningún bundle a R3.

La misma verificación se ejecutó sobre el paquete publicado por el run [`39700812531`](https://github.com/sircalch/Ostrin/actions/runs/39700812531), ligado a `main` en `bd021d242df94e58cfb4f31b97dc5ece83b3c523`. El artefacto [host-bound 12393350506](https://github.com/sircalch/Ostrin/actions/runs/39700812531/artifacts/12393350506) registró 29/29 capturas estructuralmente listas y scorecard R3=0. La reproducción independiente de esos bytes devolvió `match` para los 29 bundles y coincidencia para las evidencias archivadas de lectura/azar. Esta comprobación prueba la reproducción de ese paquete, no la ejecución independiente del replay por un job de GitHub Actions ni la completitud del contrato R3.

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
