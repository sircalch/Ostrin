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
El auditor compara además el inventario encontrado con los fixtures registrados en
`scripts/experiment-bundle.mjs`. Un directorio vacío, un bundle faltante o uno no
registrado hace fallar tanto `--require-capture` como `--require-ready`; el conteo
por sí solo no acredita que estén los archivos correctos.
El gate estructural comprueba los valores declarados del contrato WASI (`schema`, estado
parcial, target, Preview 1 y runner) y exige `host.source = github-actions`, además de
campos públicos no vacíos del runner. Esto valida consistencia con el esquema de captura;
no constituye una firma criptográfica ni una atestación independiente del origen.

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
no se guardan hostname, usuario ni rutas privadas. Cada captura registra también el repositorio,
workflow, evento, ref, número de ejecución/intento, URL de Actions y el SHA probado como
`runtime.host.ci_run`. `head_sha` identifica la punta del PR y `tested_sha` el commit que ejecutó el
workflow (en `pull_request`, normalmente el merge sintético); no se confunden entre sí.
`provenance.commit` sigue identificando el commit de los archivos del experimento. La asociación de
los SHA y la URL es metadato estructural del runner, no una firma criptográfica ni una atestación
independiente.
La acción conserva además el reporte
R3, el scorecard, la página de estado y los bundles host-bound como el artefacto
`ostrin-r3-host-evidence-<run_id>` durante 90 días, y lo sube solo después de que terminen
correctamente los replays, las comprobaciones web y los demás gates de distribución de ese job.
Si una de esas comprobaciones falla, no se publica el paquete como evidencia verificada. La
verificación local mantiene el campo ausente para que un checkout de desarrollador no se presente
como una ejecución CI.

El scorecard generado dentro de CI también publica un bloque `developmentEvidence` con el workflow,
`run_id`, `head_sha`, `tested_sha` y las URLs del run y sus artefactos. Esto permite llegar desde el
panel público al paquete host-bound que respalda sus métricas de desarrollo, sin cambiar las
revisiones históricas del candidato ni la revisión fuente del tag publicado. En una ejecución local,
el bloque queda en `null`.

La ejecución CI del head `7d80ee1609cfc059658f5b6af0f33580b9ad4e5d` completó el mismo gate con
captura auténtica: **29/29 estructuralmente READY**, 29/29 capturas de host válidas, 29/29 filas de entrada cerradas,
74 bordes simbólicos en el inventario global y cero bordes sin resolver. El workflow pasó la
verificación `--require-capture` y las pruebas de readiness/scorecard. El paquete de evidencia de
90 días está disponible como [artefacto de la ejecución 38859292510](https://github.com/sircalch/Ostrin/actions/runs/38859292510/artifacts/12028054775).
Este resultado acredita esa ejecución CI. El reporte local sigue `OPEN` porque el checkout no
debe sintetizar datos del host; el tablero publicado puede enlazar evidencia host-bound de CI, pero
el nivel `reproducibility.R3` permanece en cero mientras no se satisfaga el contrato completo.

El corte posterior de `main` en `bd6d80ebec2d53b4055e6e846609d90162ebc985` (Deploy Ostrin website,
run [39555355723](https://github.com/sircalch/Ostrin/actions/runs/39555355723)) volvió a validar
29/29 capturas estructurales, 29/29 filas cerradas, 74 bordes simbólicos globales y cero sin
resolver. Su artefacto host-bound está disponible [aquí](https://github.com/sircalch/Ostrin/actions/runs/39555355723/artifacts/12325497039).
Ese recibo todavía inventaría y reejecuta los 11 bundles R1/R2; los 18 R0 se verificaron desde el
checkout del workflow, no desde el paquete descargado. Por eso el resultado no promociona R3; la
extensión del replay R0 aún requiere sus propios checks de CI.

Una ejecución CI anterior (`38162721694`) informó `READY` para 29/29 bundles sin validar la
captura estructural de efectos ni el cierre de la fila de entrada. Ese resultado histórico no basta
bajo el contrato actual. Las capturas host-bound siguen siendo evidencia útil, pero no sustituyen
la firma de efectos del entrypoint ni las demás comprobaciones.

## Diagnósticos de brechas

`gapDiagnostics` desglosa los campos de captura ausentes o incompletos, el inventario de referencias
no resueltas y callbacks simbólicos, la fila de efectos del entrypoint y la alcanzabilidad directa
que puede reconstruirse desde `function_calls`. Las referencias por sintaxis (identificador, ruta de
miembro o ruta cualificada) son agrupaciones textuales; no deciden si una llamada resuelve a función,
método, callback o API externa. El recorrido directo sigue solo aristas que coinciden exactamente
con nombres registrados de función y no acredita alcanzabilidad dinámica completa.

En el reporte local generado desde el código actual hay 29/29 filas de `main` cerradas, 74
referencias a 10 callbacks simbólicos en el inventario global, cero llamadas no resueltas y 29/29
capturas sin `provenance.runtime.host`. El recorrido directo encuentra siete referencias a dos
callbacks en siete bundles. Esos contadores describen vistas distintas del análisis; no prueban
pureza ni sustituyen la captura de host. Por ello el reporte local continúa **OPEN**, con 0/29
bundles R3 listos.

`gapDiagnostics.reproducibilityEvidence` cuenta las declaraciones encontradas en los bundles:
snapshots embebidos con ruta/hash, etiquetas `verified-in-ci` de ejecución y estados registrados de replay, y políticas
numéricas que expresamente no afirman comparación multiplataforma. Estos contadores no vuelven a
ejecutar esos replays ni prueban que se hayan enumerado todas las entradas externas. La cantidad de
políticas que mencionan tolerancias solo mide esa mención: no indica que las tolerancias hayan sido
evaluadas. La justificación científica aún requiere métricas, unidades, método de referencia y dominio.

`gapDiagnostics.externalInputEvidence` separa las entradas declaradas como `parameters` de las
declaradas como `external-snapshot`, y cruza ruta/hash de estas últimas con `runtime.input_snapshots`.
El cruce verifica consistencia estructural del manifiesto. No prueba que no existan otras entradas
externas ni que el contenido real coincida; esa verificación corresponde al replay que consume el
snapshot. En el snapshot local hay 11 entradas externas con referencia embebida coincidente y
capacidad `filesystem.read`, y 18 entradas de parámetros; esos números tampoco cierran R3.

## Primer recibo de replay ligado a una ejecución

El fixture `surface` es el primer corte de evidencia **adicional** a la captura estructural.
Website CI ejecuta su replay R2 dentro de un directorio WASI aislado con `main.ostrin` y
`data.json`, compara el SVG completo, y comprueba que cambiar el parámetro declarado modifica
la figura. Sólo después de ese replay emite `target/r3-ci/surface-replay-receipt.json`. El
recibo liga los hashes de los cuatro archivos embebidos, el bundle completo, la fila de efectos
cerrada de `main`, el SHA de los bytes WASM cargados por cada proceso hijo y el `run_id`/`tested_sha`
capturados por el mismo job. El verificador vuelve a contrastarlos con el bundle, el binario
WASM presente y el checkout que está ejecutando CI. El artefacto host-bound incluye el recibo y
los bytes WASM construidos por ese run, para verificar el paquete descargado sin sustituirlos
por un binario local potencialmente distinto. El recibo se genera en una ruta ignorada y no se
versiona.

En un paquete descargado, `--verify-artifact` valida integridad y procedencia estructural del
recibo de `surface`; `node scripts/experiment-replay.mjs --replay-artifact-all <directorio>
--run-id <id> --tested-sha <sha>` vuelve a ejecutar el inventario R0/R1/R2 registrado en ese run con el
WASM archivado. Cada ejecución WASI se aísla en un proceso hijo con un límite de 120 segundos.
Los R0 ejecutan dos veces el `source.ostrin` archivado con sus parámetros registrados y comparan
la salida SVG completa y su hash de invocación; no se les atribuye sensibilidad de entradas
externas. Los R1/R2 comparan el SVG completo, prueban el cambio de
parámetro y, para R1, repiten la semilla. Cada captura
debe identificar el mismo run/SHA y tener una fila de entrada cerrada. El recibo registra para
cada bundle su ID, nivel y hash, de modo que selección y madurez no dependan del checkout desde el
que se inspecciona un artefacto antiguo. Cada replay contrasta los bytes con ese inventario y
verifica la fila de efectos cerrada de `main`, que es la función realmente ejecutada. El recibo de
`surface` ancla el hash del módulo que cargan todos los replays. La lectura limita el tamaño de cada archivo
y rechaza rutas que escapen del directorio descargado. Website CI ejecuta este replay completo
antes de subir el paquete como evidencia. La captura de CI no es una atestación criptográfica:
el éxito acredita las salidas bajo ese WASM y host, no equivalencia numérica universal.

`fixtureSourceRevision` identifica el commit del programa fuente `examples/viz_surface.ostrin`;
el verificador contrasta sus bytes con ese commit. `data.json` y `figure.svg` se identifican por
sus hashes actuales en el bundle y el recibo, no se atribuyen a ese commit histórico.
`ciRun.tested_sha` identifica el commit que ejecutó el workflow; en un PR puede ser el merge
sintético. `compilerWasmSha256` identifica los bytes ejecutados. Son identidades distintas: el
SHA del binario no demuestra por sí solo desde qué commit se construyó y el commit del fixture
no es la revisión de construcción del compilador.

El hijo WASI carga el binario por su cuenta. Por ello el digest se calcula **dentro del hijo**
sobre los bytes que compila, y el replay rechaza cualquier cambio de módulo entre la ejecución
base y la de sensibilidad. Un hash calculado sólo en el proceso padre no acreditaría ese enlace.

Este recibo prueba el replay y la integridad para esa ejecución, en Node WASI Preview 1 y su
host declarado. La captura de CI no es una atestación criptográfica independiente, el analizador
de efectos sigue siendo experimental, y el recibo no demuestra equivalencia numérica entre
plataformas ni completitud formal de entradas dinámicas. El scorecard público conserva **R3=0**
y los 29 bundles versionados mantienen R0/R2; este corte no promociona automáticamente `surface`
ni el inventario completo a R3.
