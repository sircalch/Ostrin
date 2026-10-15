# R3 readiness

R3 requiere una captura auditable de procedencia, hashes, lockfile, entorno,
dependencias, efectos, scheduler, entradas, parámetros y outputs. El auditor
separa los campos base, las extensiones de captura ausentes y las capturas de
efectos cuyo inventario todavía no puede cerrarse.

```text
node scripts/r3-readiness.mjs --write
```

Pages CI captura metadatos del host y valida el contrato estructural con
`node scripts/r3-readiness.mjs --require-capture`. Ese gate permite conservar una
captura incompleta para diagnóstico; **no equivale** a `--require-ready`. R3 solo
queda `READY` cuando el reporte vigente marca todos los bundles listos.

El reporte queda en [`validation/r3-readiness.json`](../validation/r3-readiness.json).
Un estado `OPEN` es deliberado y no promociona un bundle R0/R1/R2 a R3.

## Diagnóstico del snapshot actual

En el snapshot de código `29c24c0bd607822dbc2609a54bfcce545ca563f6`:

- Los 29 bundles tienen completos sus campos base.
- Ninguno está listo para R3: los 29 carecen de `provenance.runtime.host` y los 29 tienen captura de efectos incompleta.
- El inventario registra 4,266 referencias `caller-operation` sin clasificar y 130 nombres distintos. Su forma sintáctica se divide en 2,003 identificadores y 2,263 rutas de miembro.

Son referencias registradas por el inventario, no invocaciones dinámicas del programa. Una ruta como `f` puede ser un builtin, función local o callback; `data.max` puede ser un método, campo o API del host. El agrupamiento no determina resolución tipada ni demuestra pureza. `gapDiagnostics` existe para orientar el trabajo de inferencia; no cambia el gate de readiness.

Este reporte tampoco determina si las entradas externas están capturadas de forma completa y segura ni si las tolerancias numéricas están justificadas científicamente. Esos contratos requieren validación independiente.

## Captura del host en CI

La acción de Pages activa `OSTRIN_CAPTURE_HOST=1` al regenerar los bundles. Conserva metadatos públicos del runner (sistema, arquitectura, imagen, Node, WASI y hardware visible al proceso), sin hostname, usuario ni rutas privadas. El artefacto `ostrin-r3-host-evidence-<run_id>` se conserva 90 días.

El artefacto de la ejecución `38162721694` no cierra el estado actual. GitHub informa que esa ejecución fue cancelada; además, su reporte anterior marcaba `READY`, pero los bundles archivados no contenían `effect_coverage` ni `unresolved_calls`. La implementación vigente exige una captura estructuralmente válida y que el inventario de efectos esté completo. Por eso no se usa ese artefacto antiguo para promover los 29 bundles.
