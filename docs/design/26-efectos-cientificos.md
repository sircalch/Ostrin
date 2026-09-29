# 26 — Efectos científicos, determinismo y procedencia

**Estado:** propuesta de diseño; no modifica la sintaxis estable ni añade capacidades de runtime por sí sola.

**Fecha:** 2026-09-29

## 1. Propósito

Ostrin quiere que un resultado científico pueda explicar no solo qué valor produjo, sino también qué
supuestos, entradas y efectos hicieron posible ese valor. La biblioteca `std.measurements` ya conserva
incertidumbre escalar; el siguiente contrato debe hacer visibles el azar, el reloj, la E/S, la red, la
concurrencia y la procedencia que pueden cambiar una ejecución.

Un efecto no es un error ni una promesa de que el programa será reproducible. Es una parte del contrato
que permite al checker, al compilador, al runtime, al notebook y a la web responder preguntas concretas:

- ¿la función es pura o observa el mundo exterior?
- ¿un resultado aleatorio puede repetirse con una semilla y un algoritmo declarados?
- ¿qué datos, versión del compilador y política de scheduler produjeron una figura?
- ¿qué operaciones pueden reordenarse o ejecutarse de manera distinta en native y WASM?
- ¿qué información debe aparecer en un informe para que otra persona pueda auditarlo?

La propuesta separa esas preguntas sin convertir el núcleo en un teorema-prover ni ocultar efectos detrás
de magia global.

## 2. Revisión de antecedentes

| Fuente | Decisión que informa el diseño |
| --- | --- |
| [F*](https://fstar-lang.org/) y su [tutorial de efectos](https://fstar-lang.org/tutorial/book/part4/part4.html) | Los efectos pueden formar parte de tipos, contratos y refinamientos; las mónadas de Dijkstra muestran una ruta para especificar pre y postcondiciones sin confundirlas con la sintaxis del programa. |
| [Koka: evidencia y filas de efectos](https://prg.is.titech.ac.jp/papers/pdf/tfp2022adjustment.pdf) | Una fila extensible permite que una función sea polimórfica sobre efectos y que el compilador conserve evidencia de las operaciones que todavía están abiertas. |
| [W3C PROV-DM / PROV-SEM](https://www.w3.org/TR/prov-sem/prov-sem.pdf) | La procedencia describe entidades, actividades y agentes para evaluar calidad, confiabilidad y confianza; no es lo mismo que el valor científico ni que el efecto de ejecución. |
| [Definición de Reproducible Builds](https://reproducible-builds.org/docs/definition/) | La reproducibilidad fuerte requiere conservar fuente, entorno e instrucciones y comprobar que producen artefactos idénticos; no se debe prometer bit a bit entre plataformas sin evidencia. |

Estas referencias son antecedentes, no afirmaciones de novedad. La integración concreta con `Quantity`,
`Measurement<T>`, `std.viz`, el compilador en WASM y los informes de Ostrin deberá evaluarse con pruebas
antes de llamarse una contribución original.

## 3. Objetivos

1. Clasificar los efectos observables del lenguaje y hacer que el checker pueda inferirlos.
2. Separar pureza, determinismo, reproducibilidad, procedencia, mutabilidad y ownership.
3. Rechazar usos incompatibles con un contrato declarado, por ejemplo una lectura de red dentro de una
   función pura o una ejecución reproducible sin semilla para un generador aleatorio.
4. Conservar la información suficiente en HIR e IR para impedir reordenamientos que cambien la semántica.
5. Declarar las fronteras distintas de native, WASI y navegador: ningún host debe obtener red o E/S
   implícita por compilar el mismo programa.
6. Emitir un artefacto de procedencia serializable que puedan consumir la CLI, la galería Viz, los
   notebooks y los informes HTML/PDF.
7. Mantener paridad de comportamiento entre intérprete, nativo y WASM antes de publicar una capacidad.
8. Permitir que las mediciones, gráficas, animaciones, tablas y simulaciones indiquen sus supuestos y
   nivel de reproducibilidad.

## 4. No objetivos

- No implementar en esta fase una lógica dependiente completa ni probar automáticamente toda propiedad
  numérica de una función.
- No hacer que cada `Float` lleve un registro de procedencia pesado cuando el usuario no lo solicite.
- No convertir una función con efectos externos en reproducible por ocultar el reloj, la red o la E/S.
- No fijar todavía una sintaxis pública como `@effects(...)`; los ejemplos de este documento son
  notación conceptual.
- No afirmar que dos plataformas producen bits idénticos si sus bibliotecas matemáticas, ABI o
  scheduler no tienen un contrato común.
- No inferir que una función pura es científicamente correcta, estable numéricamente o libre de
  errores de modelado.
- No sustituir las capacidades del sistema operativo, los permisos del navegador ni las políticas de
  seguridad por una lista de efectos en el compilador.

## 5. Modelo conceptual

Una función tiene una firma de valores y una fila de efectos:

```text
f : (A, B) -> C ! E
E = { pure, random(seed), clock, io, network, concurrency,
      provenance, measurement, unsafe }
```

La fila es conceptual. Puede representarse como una tabla interna, un bitset extensible o tipos de
capacidad; el formato final debe conservar nombres estables, argumentos relevantes y versión de la
semántica.

### 5.1 Ejes que no deben mezclarse

- **Pura:** no observa ni modifica estado externo y no depende de una operación observable de runtime.
- **Determinista:** con las mismas entradas y el mismo contrato de runtime produce la misma salida.
- **Reproducible:** una persona puede reconstruir la entrada, el entorno y el procedimiento necesarios
  para volver a ejecutar el cálculo dentro de tolerancias declaradas.
- **Procedencia:** metadatos que describen entidades, actividades y agentes de una ejecución.
- **Mutabilidad/ownership:** reglas de aliasing y vida de valores; no son efectos científicos por sí solos.
- **Medición:** propagación de incertidumbre y fuentes correlacionadas; puede consumir datos o azar, pero
  no reemplaza la procedencia de la ejecución.

Una simulación pura puede ser determinista sin ser reproducible si se pierde su fuente de datos. Una
función con `random(seed)` puede ser reproducible aunque no sea pura. Una figura puede tener procedencia
completa y aun así contener un algoritmo numéricamente inestable.

### 5.2 Efectos mínimos

| Efecto | Significado mínimo | Datos que puede requerir |
| --- | --- | --- |
| `pure` | No hay observación o mutación externa | ninguno |
| `random(seed)` | Consume un flujo de pseudoazar identificado | semilla, algoritmo, versión, posición del flujo |
| `clock` | Observa tiempo monotónico o calendario | reloj y zona/precisión declaradas |
| `io` | Lee o escribe filesystem, stdin/stdout o dispositivo | capacidad, ruta, hashes o snapshot |
| `network` | Usa red o un servicio remoto | endpoint, respuesta y política de caché |
| `concurrency` | Usa tareas, canales, hilos o scheduler | modelo, orden relevante y política |
| `provenance` | Registra metadatos de evaluación | entidad, actividad, agente y hash |
| `measurement` | Propaga fuentes, covarianzas o modelos de incertidumbre | modelo, parámetros y sensibilidad |
| `unsafe` | Cruza una frontera sin garantías del checker | ABI, contrato externo y revisión humana |

`pure` no necesita aparecer en una fila almacenada: puede ser la fila vacía. Se conserva el nombre en
los diagnósticos para que el usuario entienda por qué una función puede o no usarse en un contexto.

## 6. Contratos de efecto

Los contratos siguientes son semántica objetivo, no API estable.

### 6.1 Pureza

Una función pura solo puede llamar funciones cuya fila sea compatible con `pure`, leer sus argumentos y
crear valores locales. El checker debe rechazar, por ejemplo, una lectura de archivo, `now()`, una red,
un RNG global o una escritura de procedencia con estado global.

La pureza no impide asignaciones locales ni garantiza que una operación de coma flotante sea exacta. El
contrato se refiere a observables externos.

### 6.2 Aleatoriedad reproducible

El azar científico debe usar una semilla y un flujo explícitos. La semilla no debe ser un singleton
oculto que dependa de la hora o de la dirección de memoria. La procedencia debe guardar:

- semilla o identificador seguro de la semilla;
- algoritmo y versión del generador;
- subflujo o contador de muestras;
- política de partición cuando hay paralelismo.

Un RNG criptográfico, una fuente de entropía del sistema o una semilla generada automáticamente conserva
el efecto `random` y no puede entrar en un contrato `@reproducible` sin un snapshot/capacidad explícita.

### 6.3 Reloj

`clock` distingue un reloj monotónico para medir duraciones de un calendario para etiquetar datos. El
compilador no debe reemplazar silenciosamente el reloj por una constante. Una prueba o notebook puede
inyectar un reloj virtual y convertir el efecto en una entrada capturada.

### 6.4 E/S y red

La E/S y la red requieren capacidades explícitas. Native puede recibir una capacidad del proceso; WASI y
el navegador deben denegarla por defecto. Una respuesta remota solo es reproducible si se conserva un
snapshot o hash verificable y la política de caché está documentada.

La ejecución web no debe subir el código, los parámetros ni los datos científicos para calcular una
figura local. Un enlace compartible puede contener el programa o un identificador público, parámetros,
cámara y hashes, pero la exportación de datos privados requiere una acción explícita del usuario.

### 6.5 Concurrencia

Debe distinguirse una ejecución cooperativa con orden definido de hilos nativos cuyo orden puede variar.
Una reducción paralela puede declarar una política asociativa/estable, una tolerancia numérica o un
scheduler capturado. El efecto `concurrency` no autoriza al optimizador a cambiar una reducción que no
cumpla el contrato matemático documentado.

### 6.6 Procedencia

Registrar procedencia no cambia el valor de una función. El registro puede ser append-only y vivir en un
artefacto de ejecución, un informe o una figura. Si el usuario desactiva la captura, la función conserva
sus otros efectos y el nivel de reproducibilidad baja; no se inventan metadatos ausentes.

### 6.7 Mediciones

`std.measurements` consume un modelo científico: estado `Exact`, `Standard` o `Unknown`, fuentes,
sensibilidades y operaciones propagadas. Una figura con barras de error debe recibir el modelo y su
método; no puede rotular un `sigma` como intervalo de confianza sin una conversión explícita.

## 7. Niveles de reproducibilidad

La CLI, el Lab y los informes pueden mostrar niveles acumulativos:

| Nivel | Garantía propuesta | Requisitos |
| --- | --- | --- |
| R0 | Repetición determinista de una función pura bajo el mismo contrato de runtime | entradas serializadas y versión de semántica |
| R1 | Replay de azar sembrado | semilla, algoritmo/versión y partición de flujos |
| R2 | Replay de entradas externas | snapshots o hashes de archivos/respuestas, capacidades y parámetros |
| R3 | Artefacto auditable | fuente, lockfile, compiler/runtime, target, plataforma, efectos, procedencia y salida |

R0–R3 describen evidencia disponible, no una escala de calidad científica. Las comparaciones entre
plataformas deben añadir tolerancias numéricas, biblioteca matemática y arquitectura. “Bit a bit” solo se
publica cuando una prueba lo verifica para ese target.

## 8. Inferencia y reglas del checker

1. Cada builtin y función de módulo tendrá una fila conocida o una declaración de frontera `unsafe`.
2. La llamada a una función tiene la unión de sus efectos y de las capacidades que consume.
3. Una función de orden superior conserva los efectos de la función recibida; no puede fingir una fila
   vacía por borrar el tipo función.
4. El retorno temprano, `match`, bucles y closures unen las filas de todos los caminos alcanzables.
5. Una función declarada pura no puede llamar una fila que contenga `random`, `clock`, `io`, `network`,
   `concurrency`, `measurement` o `unsafe` sin una regla explícita y verificable.
6. Un contrato de reproducibilidad requiere semilla para RNG, snapshot/capacidad para entradas externas,
   versión de compilador y política de scheduler cuando correspondan.
7. Un efecto explícitamente escalado debe aparecer en el diagnóstico y en la firma resuelta; no hay
   estrechamiento implícito en una asignación.
8. Las conversiones de `Measurement<T>` a `T` siguen siendo operaciones explícitas; descartar el modelo
   puede registrar un evento de procedencia o una advertencia según la política futura.

Los diagnósticos deben indicar la llamada que introdujo el efecto, la firma que lo exige y el contrato
que fue violado. El checker no debe mostrar una “prueba de reproducibilidad” si solo verificó que existe
una semilla.

## 9. HIR, IR y optimización

El compilador debe mantener una tabla de efectos por función resuelta y anotar los nodos HIR que cruzan
fronteras. La IR puede representar:

- handles de capacidad para E/S y red;
- estado de semilla/flujo para RNG;
- operaciones de reloj;
- límites de scheduler y sincronización;
- nodos de procedencia;
- propagación de mediciones y fuentes.

El lowering conserva el orden de operaciones con efectos. Un optimizador puede reordenar operaciones
puras si preserva el resultado; no puede mover una lectura de reloj, una E/S, una mutación de
procedencia o una operación de medición a través de una frontera observable sin una regla demostrada.

La tabla de efectos debe participar en `--hir`, `--ir` y el futuro informe de tipos nativos para que la
migración AST → HIR → IR no borre el contrato. Un fallback AST que no pueda demostrar una regla debe
mantener la fila conservadora y quedar visible en la evidencia del backend.

## 10. Native, WASI y navegador

| Target | Política predeterminada | Opt-in documentado |
| --- | --- | --- |
| Intérprete | Capacidades explícitas del proceso de prueba | reloj, filesystem, red y scheduler inyectados |
| Native | Sin red implícita; E/S mediante capacidades del proceso | permisos, hilos nativos, FFI y dispositivos |
| WASI | Sandbox del host y preopens declarados | directorios, argumentos y recursos otorgados por el host |
| Web/WASM | ejecución local; red y almacenamiento host denegados por defecto | APIs del navegador mediadas por el host de la aplicación |

Los programas que solo usan `pure`, `random(seed)`, `measurement` y `provenance` deben poder ejecutarse
en el Scientific Lab sin servidor. La web debe mostrar cuando una demo baja de R2 a R0/R1 porque no
incluye sus datos externos.

## 11. Procedencia y formato de artefacto

El formato inicial puede ser JSON estable, versionado y fácil de archivar:

```json
{
  "schema": "ostrin.provenance/v0",
  "program": { "source_hash": "…", "entry": "main" },
  "compiler": { "version": "…", "commit": "…", "target": "wasm32-wasi" },
  "inputs": [{ "id": "data.csv", "sha256": "…", "kind": "entity" }],
  "activity": { "effects": ["random(seed)", "measurement"], "seed": 11 },
  "runtime": { "scheduler": "cooperative", "math": "…" },
  "outputs": [{ "id": "figure.svg", "sha256": "…", "kind": "entity" }],
  "reproducibility": "R2"
}
```

Los nombres de `entity`, `activity` y `agent` siguen la idea de W3C PROV, pero el esquema no pretende
ser una implementación completa de PROV-DM. El documento debe incluir hashes y versiones que el usuario
pueda revisar, no identificadores personales por defecto. Los paths privados, tokens y respuestas
sensibles se reemplazan por hashes o referencias locales.

Una figura, tabla, video o notebook puede incrustar el JSON o un enlace a él. La salida visual debe
conservar al menos `source-hash`, `data-hash`, `seed` si existe, compilador, target y nivel R0–R3.

## 12. Visualización, animación y notebooks

`std.viz` puede consumir la fila de efectos para que un artefacto explique cómo fue generado:

- tooltips y leyendas indican unidad, modelo de medición y nivel de cobertura cuando exista;
- animaciones guardan semilla, número de frames, política temporal y versión del renderer;
- escenas 3D guardan parámetros de cámara por separado de los datos y no suben datos privados;
- tablas y figuras enlazadas conservan el mismo `data-hash` y orden de filas;
- exportaciones SVG, PNG, HTML, WebM, GIF, MP4 o PDF pueden incluir procedencia y advertir cuándo un
  codec o una captura de pantalla rompe R3;
- el Lab y un futuro Studio muestran la fila de efectos, el nivel R y las entradas faltantes antes de
  ofrecer “Copy link” o “Export report”.

Una visualización no debe presentar una animación bonita como evidencia científica si su semilla,
fuentes, incertidumbre o datos no están disponibles. La interfaz debe ser clara sin convertir el sitio
en una consola de logs.

## 13. Plan de implementación

1. **Inventario (sin sintaxis nueva):** clasificar builtins, módulos, ejemplos y fronteras FFI por efecto;
   añadir una tabla interna solo para diagnósticos.
2. **Representación interna:** definir nombres, argumentos y uniones de filas; publicar errores estables
   para llamadas puras incompatibles.
3. **HIR/IR:** propagar `pure`, `random(seed)`, `io`, `network`, `concurrency`, `provenance` y
   `measurement` en funciones resueltas y límites de optimización.
4. **Semillas y capacidades:** ofrecer handles explícitos para RNG y E/S, primero en ejemplos de prueba y
   sin alterar la semántica estable del resto del lenguaje.
5. **Artefacto de procedencia:** implementar JSON versionado, hashes de fuente/datos/salida y niveles R0–R3
   en una orden CLI experimental.
6. **Frontera WASM:** conectar la tabla con permisos del Scientific Lab; denegar red/E/S no declaradas y
   hacer visible la razón.
7. **Visualización y notebook:** incrustar procedencia en figuras, tablas y estados compartibles; añadir
   replay desde una URL o un archivo de parámetros.
8. **Revisión de sintaxis:** solo después de medir diagnósticos, paridad y ergonomía decidir si conviene
   una anotación pública, tipos de capacidad o inferencia totalmente implícita.

Cada fase debe mantener intérprete/native/WASM equivalentes para los programas que comparten target y
publicar sus límites. No se debe añadir una promesa de reproducibilidad a la portada mientras la prueba
correspondiente no exista.

## 14. Pruebas y evidencia

- inferencia positiva y negativa de filas en funciones, closures, callbacks y funciones genéricas;
- rechazo de `clock`, IO, red y RNG global en funciones puras;
- RNG sembrado con replay exacto y diagnóstico cuando cambia el algoritmo o la versión;
- capacidades de filesystem, WASI preopens y denegación de red en navegador;
- paridad de scheduler cooperativo y divergencia documentada de hilos nativos;
- serialización determinista del artefacto de procedencia sin secretos ni paths privados;
- optimizaciones que respetan fronteras de efectos en HIR/IR/C;
- propagación de `Measurement<T>` y sus fuentes en figuras y tablas;
- hashes y metadatos idénticos donde el contrato los exige, con tolerancias declaradas donde no;
- pruebas de navegador para Lab, Viz, enlaces compartibles, exportaciones y modo de movimiento reducido;
- ASan, UBSan, leak-check y fuzzing en handles de capacidades, semillas y estructuras de procedencia.

## 15. Preguntas abiertas

1. ¿La fila será visible en la firma de función, en una anotación, en un wrapper de capacidad o en una
   combinación de esas formas?
2. ¿Qué granularidad necesita `measurement`: una operación escalar, una fuente, un bloque de covarianza o
   una política de propagación completa?
3. ¿Debe la procedencia ser implícita para todos los artefactos o activarse por módulo/CLI/notebook?
4. ¿Cómo se versiona un RNG cuando se corrige un algoritmo pero se desea conservar replay histórico?
5. ¿Qué operaciones de `concurrency` pueden declarar asociatividad, orden estable o tolerancia numérica?
6. ¿Qué campos de W3C PROV conviene mapear directamente y cuáles deben seguir siendo específicos de Ostrin?
7. ¿Cómo se compone una capacidad a través de paquetes y FFI sin convertir la frontera en una lista opaca?
8. ¿Qué política de privacidad se aplica a hashes, endpoints, nombres de archivo y agentes en informes
   públicos?
9. ¿Qué contrato de plataforma permite afirmar bit a bit entre native, WASI y WebAssembly, y cuándo basta
   una tolerancia científica?

## 16. Decisiones aplazadas

- No se fija todavía una sintaxis `@pure`, `@effects` o `@reproducible`.
- No se añade una variable global de semilla ni un reloj global reproducible por defecto.
- No se declara que un resultado sea reproducible solo porque compila o porque tiene una semilla.
- No se mezclan procedencia, mediciones y ownership en un único tipo pesado.
- No se promete soporte completo de W3C PROV, F* o Koka; se toman ideas verificables y se conservan sus
  límites.
- No se anuncia esta propuesta como capacidad disponible hasta que exista implementación, paridad,
  pruebas y una superficie web con evidencia.
