# R3 readiness

R3 requiere un artefacto auditable: procedencia, hashes, lockfile, entorno,
dependencias, efectos, scheduler, inputs, parámetros y outputs. El auditor
recorre los bundles publicados y separa los campos ya presentes de las
extensiones que todavía faltan.

```text
node scripts/r3-readiness.mjs --write
```

Pages CI captura metadatos del runner y usa `node scripts/r3-readiness.mjs
--require-capture`; este modo valida que la captura esté completa y
estructuralmente válida. No marca el nivel R3 como listo por tener esos metadatos.

El reporte queda en [`validation/r3-readiness.json`](../validation/r3-readiness.json).
Un estado `OPEN` es deliberado: documenta el trabajo restante y no promociona
un bundle R0/R1/R2 a R3.

El checkout local tiene los **29/29 bundles con los campos base completos**, incluidos
`execution`, dependencias y scheduler declarado. Los **0/29** locales están listos para R3
porque no deben falsificar la captura del host. Los 29 bundles ya incluyen un resumen normalizado del
reporte de efectos del compilador (`ostrin.effect-report/v0`), con hashes del analizador y
la marca de que el análisis es experimental y conservador. El resumen conserva las llamadas locales,
las llamadas no resueltas y los bordes simbólicos; el auditor valida su consistencia y reporta por
separado los bundles con inventario parcial, los bordes simbólicos y las llamadas sin clasificar.
`effect_coverage.complete` significa que no quedan llamadas observadas sin clasificar; no demuestra
pureza ni cierra R3. Un callback simbólico mantiene el bundle incompleto para R3 aunque esté
representado en el reporte. Una captura ausente o inconsistente también mantiene R3 `OPEN`. El host
sigue siendo un requisito explícito del auditor y no se rellena con valores sintéticos.

Pages CI activa `OSTRIN_CAPTURE_HOST=1` después de preparar el compilador WASI y
regenera los 29 bundles. En ese modo cada snapshot registra solo metadatos públicos
del runner (`RUNNER_OS`, arquitectura, imagen, Node, WASI y hardware visible del proceso);
no se guardan hostname, usuario ni rutas privadas. La acción conserva además el reporte
R3, el scorecard, la página de estado y los bundles host-bound como el artefacto
`ostrin-r3-host-evidence-<run_id>` durante 90 días. La verificación local mantiene el
campo ausente para que un checkout de desarrollador no se presente como una ejecución CI.

Una ejecución CI anterior (`38162721694`) informó `READY` para 29/29 bundles con
el criterio de efectos previo, que solo consultaba `effect_coverage.complete`. Ese resultado
histórico no basta bajo el criterio actual: los callbacks simbólicos también deben cerrarse antes
de declarar R3 listo. Las capturas host-bound siguen siendo evidencia útil, pero no sustituyen
ese cierre.
