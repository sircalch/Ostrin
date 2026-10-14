# R3 readiness

R3 requiere un artefacto auditable: procedencia, hashes, lockfile, entorno,
dependencias, efectos, scheduler, inputs, parámetros y outputs. El auditor
recorre los bundles publicados y separa los campos ya presentes de las
extensiones que todavía faltan.

```text
node scripts/r3-readiness.mjs --write
```

Pages CI usa además `node scripts/r3-readiness.mjs --require-ready`; ese modo
termina con error si cualquier bundle no tiene todos los campos R3 o una captura
de efectos estructuralmente válida.

El reporte queda en [`validation/r3-readiness.json`](../validation/r3-readiness.json).
Un estado `OPEN` es deliberado: documenta el trabajo restante y no promociona
un bundle R0/R1/R2 a R3.

El checkout local tiene los **29/29 bundles con los campos base completos**, incluidos
`execution`, dependencias y scheduler declarado. Los **0/29** locales están listos para R3
porque no deben falsificar la captura del host. Los 29 bundles ya incluyen un resumen normalizado del
reporte de efectos del compilador (`ostrin.effect-report/v0`), con hashes del analizador y
la marca de que el análisis es experimental y conservador. El resumen conserva las llamadas locales,
las llamadas no resueltas y el conteo de cobertura; el auditor valida su consistencia y reporta por
separado cuántos bundles tienen inventario parcial y cuántas llamadas siguen sin clasificar. Una captura
coherente puede estar lista para auditoría R3 aunque su inventario sea parcial; `complete` significa que
no quedan llamadas sin clasificar, no que el programa sea puro. Una captura ausente o inconsistente
mantiene R3 `OPEN`. El host sigue siendo un requisito explícito del auditor y no se rellena con valores sintéticos.

Pages CI activa `OSTRIN_CAPTURE_HOST=1` después de preparar el compilador WASI y
regenera los 29 bundles. En ese modo cada snapshot registra solo metadatos públicos
del runner (`RUNNER_OS`, arquitectura, imagen, Node, WASI y hardware visible del proceso);
no se guardan hostname, usuario ni rutas privadas. La acción conserva además el reporte
R3, el scorecard, la página de estado y los bundles host-bound como el artefacto
`ostrin-r3-host-evidence-<run_id>` durante 90 días. La verificación local mantiene el
campo ausente para que un checkout de desarrollador no se presente como una ejecución CI.

La ejecución CI `38162721694` produjo el artefacto host-bound y su reporte confirma
`READY`, `29/29` bundles preparados y `openCount=0`. Esa evidencia permite auditar R3
sin convertir el checkout local en un snapshot sintético.
