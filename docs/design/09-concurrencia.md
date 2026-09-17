# Ostrin — Diseño core: Concurrencia

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [04-errores-y-result.md](04-errores-y-result.md), [06-rangos-e-iteradores.md](06-rangos-e-iteradores.md)

Decisiones de fondo ya cerradas:
- **Tareas + canales** (estilo CSP/Go), no modelo de actores.
- **Sin colorear funciones**: no existe `async fn`/`await` contagioso. `spawn` se llama desde cualquier función normal.
- **Prohibido compartir un binding `mut` directamente entre tareas.** Toda comunicación de datos que cambian pasa por canales.

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

- `spawn_scope { ... }` garantiza que **ninguna tarea lanzada dentro del bloque sigue viva al salir de él** — si el bloque termina (normalmente o por panic) con tareas todavía sin `.join()`, el propio `spawn_scope` espera a que terminen (o las cancela, según se decida en el diseño de cancelación, pendiente en §5) antes de propagar la salida.
- Se recomienda `spawn_scope` como la forma por defecto de paralelizar trabajo (por ejemplo, repartir un cálculo científico entre N tareas y esperar todos los resultados); `spawn` suelto queda para el caso explícito de una tarea de fondo de vida más larga que el scope que la creó (un logger, un servidor).

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

1. **Cancelación de tareas**: si `spawn_scope` debe poder cancelar tareas hijas activamente (no solo esperarlas) cuando una falla o el scope se interrumpe — pendiente de diseño concreto.
2. **Selección sobre múltiples canales** (`select` estilo Go, para reaccionar al primero de varios canales que tenga un valor disponible) — no cubierto en este documento.
3. **Relación con E/S**: si operaciones de E/S (leer un archivo, una petición de red) bloquean la tarea completa o se manejan con un mecanismo de espera eficiente a nivel de runtime — es una decisión de implementación del runtime más que del lenguaje, pero afecta si `spawn` es "barato" de usar en masa (miles de tareas) o no.
4. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
