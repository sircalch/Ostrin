# Ostrin — Diseño core: Concurrencia

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [04-errores-y-result.md](04-errores-y-result.md), [06-rangos-e-iteradores.md](06-rangos-e-iteradores.md)

Decisiones de fondo ya cerradas:
- **Tareas + canales** (estilo CSP/Go), no modelo de actores.
- **Sin colorear funciones**: no existe `async fn`/`await` contagioso. `spawn` se llama desde cualquier función normal.
- **Prohibido compartir un binding `mut` directamente entre tareas.** Toda comunicación de datos que cambian pasa por canales.

Estado de implementación: el intérprete ejecuta `spawn` con un scheduler cooperativo
determinista. El backend C conserva ese modo por defecto para mantener la paridad reproducible,
y añade `--native-threads` como modo explícito de ejecución real: cada `spawn` arranca un
hilo del sistema operativo, `join` espera con mutex/condición y los canales usan un buffer
protegido con espera bloqueante y despiertan a los receptores al enviar o cerrar. La API del
lenguaje no cambia y E1100/E1101 siguen aplicándose. `select` ya está implementado y
`Task.cancel()` cubre la cancelación segura de tareas pendientes y solicita cancelación
cooperativa a tareas que ya están ejecutándose; la cancelación solo se observa en puntos
seguros, incluidos los intervalos de una espera bloqueante de canal. Los grupos nativos
propagan ya la solicitud y drenan sus tareas hijas.

La idea central de este documento es que Ostrin **no necesita un borrow checker al estilo Rust** para ser seguro en concurrencia, porque ya partimos de "inmutable por defecto" (documento 01). Un dato inmutable nunca puede tener una condición de carrera — no importa cuántas tareas lo lean a la vez. El único lugar donde hace falta una regla especial es en el manejo de datos `mut`, y ahí basta una regla simple y local (no un sistema de ownership/lifetimes que atraviese todo el lenguaje).

---

## 1. `spawn` y `Task<T>`

```ostrin
task = spawn {
    heavy_computation(dataset)
}

result = task.join()
```

- `spawn { cuerpo }` lanza el bloque como una tarea concurrente y devuelve inmediatamente un `Task<T>`, donde `T` es el tipo del valor que produce el bloque (igual que un bloque `{ }` normal, la última expresión es el valor).
- `task.join()` bloquea la tarea actual hasta que `task` termine, y devuelve su resultado. Si el cuerpo de la tarea hace panic, `.join()` propaga ese panic en quien lo llama (un panic nunca desaparece en silencio, ver documento 04, §3).
- Si el cuerpo de la tarea devuelve `Result<T, E>`, `task.join()` simplemente devuelve ese `Result<T, E>` — no hay un mecanismo especial de "captura de errores concurrentes"; el manejo de errores es el mismo `Result` de siempre (documento 04), solo que el valor llegó desde otra tarea.

### 1.1 Qué puede capturar un `spawn`

```ostrin
dataset = load_dataset()          // inmutable

task = spawn {
    process(dataset)              // OK: 'dataset' es inmutable, se puede compartir sin riesgo
}
```

```ostrin
mut counter = 0

task = spawn {
    counter = counter + 1         // Error de compilación
}
```
```text
Error OSTRIN-E1100
Cannot capture mutable binding 'counter' in 'spawn'.
Mutable state cannot be shared directly between tasks.
Send it through a channel instead.
```

**Regla exacta**: un valor se puede capturar en un `spawn` (o enviar por un canal, §2) libremente si su tipo **no contiene ningún campo `mut` en ningún nivel de anidamiento** — un `Int`, un `record` con todos sus campos inmutables, un `Quantity<D>`, una `List<T>` de elementos inmutables, todos califican. Si el tipo contiene aunque sea un campo `mut` en algún nivel, no se puede capturar directamente: hay que pasarlo por un canal.

Esto reemplaza por completo la necesidad de un análisis de préstamos (`borrow checking`) como el de Rust: no hace falta rastrear referencias ni tiempos de vida, porque lo único que nunca se puede compartir sin más es precisamente lo que puede cambiar — y eso se detecta con una comprobación de tipos simple, no con un análisis de flujo de todo el programa.

## 2. Canales

```ostrin
ch = channel<Int>()                 // sin buffer (rendezvous): send bloquea hasta que alguien reciba
ch = channel<Int>(capacity: 10)      // con buffer: send no bloquea mientras haya espacio
```

```ostrin
producer = spawn {
    for i in 1 to 10 {
        ch.send(i)
    }
    ch.close()
}

for value in ch {
    print(value)
}
```

- `ch.send(value)`: envía `value` por el canal. Si `value` es de un tipo con campos `mut`, enviarlo **mueve** el binding: después de `ch.send(counter)`, usar `counter` de nuevo en la tarea emisora es error de compilación ("`counter` ya fue enviado por el canal"). Si `value` es inmutable, no hay restricción — se puede seguir usando después de enviarlo (conceptualmente se "copia" al canal, aunque la implementación pueda compartir la representación interna sin que el programador lo note, precisamente porque es inmutable).
- `ch.receive() -> Option<T>`: bloquea hasta recibir un valor (`Some(value)`) o hasta que el canal se cierre y esté vacío (`None`).
- Para un valor mutable, el envío transfiere la referencia al canal y `Some(value)` la transfiere al binding del receptor. El receptor puede usar el valor; la guardia dinámica solo cubre el intervalo en vuelo y no convierte el objeto recibido en "movido". Los aliases que quedaron en el emisor siguen siendo inválidos por E1101 estático.
- `ch.close()`: marca el canal como cerrado. Enviar después de cerrado es panic (error de programación, no un caso esperado — igual que escribir en un archivo ya cerrado).
- Un `Channel<T>` implementa `Iterator<Option<T>>`-como-protocolo (documento 06, §3) de forma que `for value in ch { ... }` recibe repetidamente hasta que el canal se cierra, reutilizando el mismo protocolo de iteración de listas y rangos sin un mecanismo aparte.

### 2.1 Reasignación tras mover un `mut` a un canal

```ostrin
mut buffer = load_large_buffer()
ch.send(buffer)
process(buffer)     // Error: 'buffer' fue movido en la línea anterior
```
```text
Error OSTRIN-E1101
'buffer' was moved into 'ch.send(...)' at line 2 and cannot be used afterwards.
```

Esta es la única forma de seguimiento de "movido" que existe en Ostrin, y solo aplica a valores mutables enviados por un canal o capturados en un `spawn` — no es un sistema de ownership general como en Rust (los bindings inmutables, que son el caso por defecto y la mayoría del código, nunca están sujetos a esta regla).

### 1.2 Cancelación segura en puntos cooperativos

```ostrin
task = spawn { expensive_step() }
if task.cancel() {
    print("cancelled before start")
}
```

`task.cancel() -> Bool` cambia inmediatamente una tarea `Pending` a `Cancelled` y devuelve
`true`. Si la tarea ya está `Running`, registra una solicitud y también devuelve `true`;
la tarea la observa en el siguiente límite seguro y termina sin continuar con la siguiente
operación. Devuelve `false` para tareas ya terminadas o canceladas. La cancelación nunca
interrumpe código arbitrario a mitad de una operación: `join()` sobre una tarea cancelada
produce un error de runtime.

En el intérprete, cada frontera de sentencia es un punto de comprobación. En el backend C
cooperativo, `yield()` y las esperas de `select` son puntos explícitos de comprobación; en
`--native-threads`, la misma operación permite salir de forma cooperativa del callback
actual, sin prometer preempción arbitraria ni detener código que no ceda el control.
Cuando se cancela una tarea que tiene `spawn_scope` activo, sus grupos anidados reciben la
solicitud inmediatamente y cancelan sus tareas hijas; el scope termina de drenar esas
tareas antes de liberar su frame. En el intérprete, una recepción vuelve al scheduler; en
`--native-threads`, una recepción vacía usa una espera temporizada corta, libera el mutex
y alcanza el checkpoint antes de volver a esperar. Así una cancelación no deja dormida
indefinidamente a una tarea en un canal; la operación sigue siendo cooperativa y no
interrumpe E/S arbitraria del sistema operativo.

### 2.2 Backend nativo con hilos reales

El backend C ofrece dos modos deliberados:

- sin `--native-threads`: scheduler cooperativo determinista, usado por la paridad
  intérprete↔nativo y por las pruebas diferenciales;
- con `--native-threads`: `pthread` en POSIX y `CreateThread` en Windows, mutexes y
  variables de condición para `Task<T>`, y canales bloqueantes con buffer dinámico.

El flag solo es válido con `--emit-c` o `--compile`. Las tareas nativas conservan su
entorno capturado mientras el hilo puede usarlo y lo liberan al terminar; el runtime de
memoria protege su tabla global contra accesos concurrentes. El registro de tareas usa
además un mutex propio: cada nodo conserva viva su tarea mientras está registrada,
el scheduler toma una referencia temporal durante el polling, y `join`/el drenado del
scope retiran nodos terminados antes de liberar sus referencias. Esta primera entrega
cubre `spawn`, `join`, `send`, `receive`, `close`, `select` y la cancelación cooperativa
en puntos seguros, incluida la propagación a grupos activos de `spawn_scope`. Las esperas
de canal son cancelables mediante un checkpoint temporizado; no promete preempción de
hilos que nunca alcanzan un checkpoint ni interrupción segura de una E/S arbitraria
bloqueante.

### 2.3 Selección determinista entre canales

```ostrin
first = select([updates, shutdown])
match first {
    Some(value) => print(value),
    None => print("closed")
}
```

`select(channels: List<Channel<T>>) -> Option<T>` inspecciona los canales en el orden
de la lista. Devuelve el primer valor disponible; un canal cerrado y vacío está listo y
produce `None`. Si ningún canal está listo, el intérprete y el backend nativo cooperativo
avanzan una tarea pendiente. Con `--native-threads`, el backend usa una operación de
recepción no bloqueante protegida por mutex por canal y cede el hilo entre intentos.
Una lista vacía es un error de ejecución. El checker exige estáticamente una lista
homogénea de `Channel<T>`, y la semántica de prioridad queda así reproducible entre
intérprete, nativo cooperativo y nativo con hilos.

`yield() -> Void` cede explícitamente el turno. En el intérprete y el backend cooperativo
ejecuta como máximo una tarea pendiente; en `--native-threads` llama a la cesión del
sistema operativo. Tanto `yield()` como la espera de `select` alcanzan el checkpoint de
cancelación después de liberar sus temporales; son herramientas de coordinación, no una
garantía de fairness ni una interrupción forzada de código arbitrario.

### 2.4 E/S externa y cancelación

`read_file(path)` y `write_file(path, contents)` devuelven `Result` y cruzan el backend nativo
mediante la misma libc/WASI del host. El runtime hace un checkpoint antes de entrar en
`fopen`/`fread`/`fputs`/`fclose` y comprueba todos los errores de seek, lectura, escritura y cierre.
Una solicitud de cancelación que llegue durante una de esas llamadas no intenta interrumpirla:
la operación sigue bloqueando el hilo que la ejecuta y la cancelación se observa cuando la libc
regresa y se alcanza el siguiente punto seguro. Esta es la semántica actual tanto en el scheduler
cooperativo como con `--native-threads`; un worker de E/S o una espera WASI especializada queda
como una decisión posterior del runtime.

## 3. Concurrencia estructurada — `spawn_scope`

Un `spawn` "suelto" puede quedar corriendo en segundo plano si nadie llama `.join()` — útil a veces, pero también una fuente común de bugs ("tareas huérfanas" que seguían vivas sin que nadie se acordara). Para el caso común de "lanzar varias tareas y esperar a que todas terminen antes de seguir", Ostrin ofrece un bloque que lo garantiza:

```ostrin
results = spawn_scope {
    a = spawn { compute_a() }
    b = spawn { compute_b() }
    c = spawn { compute_c() }
    [a.join(), b.join(), c.join()]
}
```

- `spawn_scope { ... }` garantiza que **ninguna tarea lanzada dentro del bloque sigue viva al salir de él** — si el bloque termina normalmente, espera a que terminen; si el cuerpo o la tarea propietaria se cancela, propaga la solicitud a los grupos anidados, cancela sus tareas hijas y drena el scope antes de propagar la salida.
- Se recomienda `spawn_scope` como la forma por defecto de paralelizar trabajo (por ejemplo, repartir un cálculo científico entre N tareas y esperar todos los resultados); `spawn` suelto queda para el caso explícito de una tarea de fondo de vida más larga que el scope que la creó (un logger, un servidor).

La sincronización del registro se complementa con frames de ownership para los bloques
anidados que generan handles de tarea: el emisor conserva el resultado del bloque,
libera sus bindings locales al salir y el smoke test nativo termina con cero
asignaciones vivas. La bajada completa de ownership sobre la IR —incluyendo todos los
escapes, loops y la propagación completa del control de cancelación— sigue siendo una etapa
posterior; el caso directo `Task.cancel()` para handles representables en la IR ya usa el helper
tipado del runtime, y `yield()` comparte el polling cooperativo o la espera del backend de hilos
con su checkpoint de cancelación. `select(List<Channel<T>>)` con payload soportado también
reutiliza la prioridad determinista y el checkpoint del runtime; las formas indirectas siguen
en el fallback verificado.

## 4. Ejemplo completo — map paralelo

```ostrin
fn parallel_map<T, U>(items: List<T>, f: fn(T) -> U) -> List<U> {
    spawn_scope {
        tasks = items.map(fn(item) { spawn { f(item) } })
        tasks.map(fn(t) { t.join() })
    }
}

masses = parallel_map(particles, fn(p) { compute_mass(p) })
```

Como `T` y `U` no están restringidos a ser inmutables aquí, esta función solo compila cuando `T` (lo que captura cada `spawn`) no contiene campos `mut` — que es la inmensa mayoría de los casos de datos que se procesan en paralelo (transformar una lista de valores en otra), y el compilador lo señala igual que en el ejemplo de §1.1 si alguien intenta usarla con un `T` mutable.

---

## 5. Preguntas abiertas para la siguiente sesión de diseño

1. **Cancelación de tareas**: resuelto con grupos explícitos, propagación a scopes anidados,
   checkpoints cooperativos y espera temporizada de canales; la cancelación no intenta
   preemptar código arbitrario ni E/S externa.
2. **Relación con red y E/S masiva**: los archivos locales ya tienen un contrato explícito de
   checkpoint en el límite, `Result` y bloqueo honesto durante libc/WASI. Sigue abierta la decisión
   de mover E/S lenta o de red a workers/esperas eficientes para que miles de tareas no retengan
   hilos nativos mientras esperan al sistema operativo.
3. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
