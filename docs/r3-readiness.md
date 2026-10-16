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
estructuralmente válida. Los metadatos de host por sí solos no bastan: R3 también
requiere el contrato completo del bundle y una fila de efectos cerrada para el entrypoint.

El reporte queda en [`validation/r3-readiness.json`](../validation/r3-readiness.json).
Un estado `OPEN` es deliberado: documenta el trabajo restante y no promociona
un bundle R0/R1/R2 a R3.

El checkout local tiene los **29/29 bundles con los campos base completos**, incluidos
`execution`, dependencias y scheduler declarado. Los **0/29** locales están listos para R3
porque no deben falsificar la captura del host. Los 29 bundles ya incluyen un resumen normalizado del
reporte de efectos del compilador (`ostrin.effect-report/v0`), con hashes del analizador y
la marca de que el análisis es experimental y conservador. El resumen conserva las llamadas locales,
las llamadas no resueltas, los bordes simbólicos y la firma calculada para `main`.
`entry_effect_signature` es la evidencia del cierre de efectos del punto de entrada: debe ser válida,
estar cerrada y no afirmar pureza. Una fila cerrada puede incluir efectos conocidos como `io` o
`state(local)`. Los callbacks simbólicos de funciones no alcanzadas o sustituidos por una función
conocida permanecen en el inventario global y se muestran como diagnóstico, pero no bloquean por sí
solos el contrato del entrypoint. Un callback desconocido que llegue a `main` mantiene abierta su fila.
`effect_coverage.complete` describe el inventario global de bordes no resueltos; no demuestra pureza ni
sustituye la firma del entrypoint. La captura ausente o inconsistente mantiene R3 `OPEN`. El host sigue
siendo un requisito explícito del auditor y no se rellena con valores sintéticos.

En el snapshot local del head `7d80ee1609cfc059658f5b6af0f33580b9ad4e5d`, las 29 firmas de `main`
están cerradas; el inventario agregado contiene 74 bordes simbólicos y cero bordes sin resolver.
El reporte versionado permanece **0/29 READY** porque los bundles locales no deben fingir una captura
de host.

Pages CI activa `OSTRIN_CAPTURE_HOST=1` después de preparar el compilador WASI y
regenera los 29 bundles. En ese modo cada snapshot registra solo metadatos públicos
del runner (`RUNNER_OS`, arquitectura, imagen, Node, WASI y hardware visible del proceso);
no se guardan hostname, usuario ni rutas privadas. La acción conserva además el reporte
R3, el scorecard, la página de estado y los bundles host-bound como el artefacto
`ostrin-r3-host-evidence-<run_id>` durante 90 días. La verificación local mantiene el
campo ausente para que un checkout de desarrollador no se presente como una ejecución CI.

La ejecución CI del head `7d80ee1609cfc059658f5b6af0f33580b9ad4e5d` completó el mismo gate con
captura auténtica: **29/29 READY**, 29/29 capturas de host válidas, 29/29 filas de entrada cerradas,
74 bordes simbólicos en el inventario global y cero bordes sin resolver. El workflow pasó la
verificación `--require-capture` y las pruebas de readiness/scorecard. El paquete de evidencia de
90 días está disponible como [artefacto de la ejecución 38859292510](https://github.com/sircalch/Ostrin/actions/runs/38859292510/artifacts/12028054775).
Este resultado acredita esa ejecución CI; el reporte local y el tablero estático versionado siguen
`OPEN` hasta una ejecución de publicación desde `main` que regenere y despliegue la evidencia.

Una ejecución CI anterior (`38162721694`) informó `READY` para 29/29 bundles sin validar la
captura estructural de efectos ni el cierre de la fila de entrada. Ese resultado histórico no basta
bajo el contrato actual. Las capturas host-bound siguen siendo evidencia útil, pero no sustituyen
la firma de efectos del entrypoint ni las demás comprobaciones.
