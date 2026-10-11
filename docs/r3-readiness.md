# R3 readiness

R3 requiere un artefacto auditable: procedencia, hashes, lockfile, entorno,
dependencias, efectos, scheduler, inputs, parámetros y outputs. El auditor
recorre los bundles publicados y separa los campos ya presentes de las
extensiones que todavía faltan.

```text
node scripts/r3-readiness.mjs --write
```

Pages CI usa además `node scripts/r3-readiness.mjs --require-ready`; ese modo
termina con error si cualquier bundle no tiene todos los campos R3.

El reporte queda en [`validation/r3-readiness.json`](../validation/r3-readiness.json).
Un estado `OPEN` es deliberado: documenta el trabajo restante y no promociona
un bundle R0/R1/R2 a R3.

El estado actual tiene los **29/29 bundles con los campos base completos**, incluidos
`execution`, dependencias y scheduler declarado. Los **0/29** están listos para R3 todavía:
falta la captura auditable del host. Los 29 bundles ya incluyen un resumen normalizado del
reporte de efectos del compilador (`ostrin.effect-report/v0`), con hashes del analizador y
la marca de que el análisis es experimental y conservador. El host sigue siendo un requisito
explícito del auditor y no se rellena con valores sintéticos.

Pages CI activa `OSTRIN_CAPTURE_HOST=1` después de preparar el compilador WASI y
regenera los 29 bundles. En ese modo cada snapshot registra solo metadatos públicos
del runner (`RUNNER_OS`, arquitectura, imagen, Node, WASI y hardware visible del proceso);
no se guardan hostname, usuario ni rutas privadas. La acción conserva además el reporte
R3, el scorecard, la página de estado y los bundles host-bound como el artefacto
`ostrin-r3-host-evidence-<run_id>` durante 90 días. La verificación local mantiene el
campo ausente para que un checkout de desarrollador no se presente como una ejecución CI.
