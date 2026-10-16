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

Son referencias registradas por el inventario, no invocaciones dinámicas del programa. Una ruta como `f` puede ser un builtin, función local o callback; `data.max` puede ser un método, campo o API del host. El agrupamiento no determina resolución tipada ni demuestra pureza. Cuando la captura incluye `symbolic_effect_calls`, el reporte cuenta esos callbacks en un inventario separado; no los mezcla con llamadas realmente no resueltas.

`entrypointEffects` presenta aparte los bordes abiertos y callbacks simbólicos registrados en la función de entrada. Si falta cualquiera de esos mapas, el estado es `unavailable`; la evidencia ausente nunca se interpreta como cierre. Este resumen es diagnóstico y **no cambia el gate R3**, que sigue evaluando la evidencia completa del bundle. Un cierre de la fila de entrada tampoco acredita por sí solo alcanzabilidad dinámica, captura del host ni reproducibilidad integral.

En el snapshot `29c24c0bd607822dbc2609a54bfcce545ca563f6`, las capturas no incluyen el mapa simbólico requerido para este diagnóstico, por lo que el cierre de la entrada queda `unavailable`. El avance de efectos tipados en #186 registra la fila de entrada y permite evaluar esa dimensión al integrarse; no se infiere desde el inventario global de funciones de una biblioteca.

En una auditoría separada de los bundles del head `8dd2d54b6ef08429444441bd8ae6cd4f40acbb16` de #186, los 29 mapas de entrada tienen cero llamadas abiertas y cero callbacks simbólicos, mientras el inventario global conserva 74 callbacks simbólicos. Esos bordes globales y las filas de entrada son mediciones distintas; el resultado no cambia el estado R3 ni demuestra por sí solo que todas las rutas dinámicas estén capturadas.

Este reporte tampoco determina si las entradas externas están capturadas de forma completa y segura ni si las tolerancias numéricas están justificadas científicamente. Esos contratos requieren validación independiente.

## Captura del host en CI

La acción de Pages activa `OSTRIN_CAPTURE_HOST=1` al regenerar los bundles. Conserva metadatos públicos del runner (sistema, arquitectura, imagen, Node, WASI y hardware visible al proceso), sin hostname, usuario ni rutas privadas. El artefacto `ostrin-r3-host-evidence-<run_id>` se conserva 90 días.

El artefacto de la ejecución `38162721694` no cierra el estado actual. GitHub informa que esa ejecución fue cancelada; además, su reporte anterior marcaba `READY`, pero los bundles archivados no contenían `effect_coverage` ni `unresolved_calls`. La implementación vigente exige una captura estructuralmente válida y que el inventario de efectos esté completo. Por eso no se usa ese artefacto antiguo para promover los 29 bundles.
