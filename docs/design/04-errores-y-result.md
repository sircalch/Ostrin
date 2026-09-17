# Ostrin — Diseño core: Manejo de errores, Result y Option

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [03-traits.md](03-traits.md)

Decisiones de fondo ya cerradas:
- **Panics para bugs, `Result` para errores esperables** (misma separación que Rust/Go).
- **`E` libre**: cualquier tipo puede usarse como error en `Result<T, E>`, sin trait `Error` obligatorio.
- **Propagación explícita con la palabra clave `try`** (prefijo, no operador `?` postfijo) — decisión deliberada de no replicar la sintaxis de Rust; ver §2 para el razonamiento.

---

## 1. `Option<T>` y `Result<T, E>`

```ostrin
enum Option<T> {
    Some(T)
    None
}

enum Result<T, E> {
    Ok(T)
    Err(E)
}
```

- `Option<T>` reemplaza `null` (ya introducido en el documento 01). Se usa cuando "ausencia de valor" no es un error, es un resultado válido (buscar en una lista y no encontrar nada no es un fallo del programa).
- `Result<T, E>` se usa cuando la ausencia de valor **es** un fallo con una causa que vale la pena describir (archivo no encontrado, texto no parseable, red caída).

```ostrin
fn find(items: List<Int>, target: Int) -> Option<Int> {
    ...
}

fn parse_int(text: String) -> Result<Int, String> {
    ...
}
```

## 2. Propagación con `try`

```ostrin
fn load_config(path: String) -> Result<Config, String> {
    text = try read_file(path)
    parsed = try parse(text)
    Ok(parsed)
}
```

- `try <expr>` exige que `expr` sea `Result<T, E>` (o `Option<T>`, ver más abajo) y que la función que contiene el `try` devuelva un `Result` (u `Option`) compatible.
- Si `expr` es `Ok(v)` → la expresión completa vale `v`, la ejecución sigue.
- Si `expr` es `Err(e)` → la función actual retorna inmediatamente `Err(e)`, sin ejecutar el resto del cuerpo.
- Con `Option`: `try` sobre un `Option<T>` dentro de una función que devuelve `Option<U>` retorna `None` inmediatamente si el valor es `None`, o desempaqueta `T` si es `Some(v)`.
- **No se mezclan `Result` y `Option` implícitamente**: no se puede hacer `try` de un `Option` dentro de una función que devuelve `Result`, ni viceversa. Para convertir, se usa explícitamente `.ok_or(error)` (`Option<T>` → `Result<T, E>`) o `.ok()` (`Result<T, E>` → `Option<T>`, descartando el error). La conversión siempre es visible en el código, nunca automática.

### ¿Por qué `try` como prefijo y no `?` como sufijo?

Rust usa `value = parse(text)?`. Ostrin usa `value = try parse(text)`. La diferencia no es solo estética:

- Un símbolo de un carácter al final de una línea larga es fácil de pasar por alto al leer o revisar código — justo el tipo de descuido costoso en código científico donde auditar "¿qué puede fallar aquí?" importa.
- `try` al principio de la expresión se lee en el mismo orden en que se piensa: "intenta esto, y si falla, propaga" — antes de leer qué es "esto".
- Evita que el manejo de errores dependa de un carácter que se puede omitir por error de tipeo sin que sea obvio a simple vista (`parse(text)` sin `?` sigue siendo sintácticamente válido en Rust si el contexto lo permite; en Ostrin, olvidar `try` sobre un `Result` sin usarlo de alguna forma es un valor sin consumir, y el compilador avisa — ver §5).

### Conversión de tipo de error dentro de una cadena de `try`

Como `E` es libre y no hay trait `Error` unificado, si una llamada interna falla con un tipo de error distinto al que devuelve la función contenedora, hace falta decir explícitamente cómo convertir — Ostrin no lo infiere en silencio (a diferencia de la conversión automática vía `From` que usa Rust con `?`, que consideramos parte de lo que no queremos replicar: es útil pero oculta una conversión no trivial detrás de un símbolo de una letra).

```ostrin
fn load_config(path: String) -> Result<Config, String> {
    text = try read_file(path) catch fn(e) { "IO error: " + e.message() }
    parsed = try parse(text)
    Ok(parsed)
}
```

- `try <expr> catch fn(e) { ... }`: si `expr` es `Err(e)`, ejecuta el closure `catch` con `e` disponible, y el **resultado de ese closure** es el valor que se retorna envuelto en `Err(...)`. Debe producir un valor del tipo `E` que la función exterior declara. `catch` toma un closure normal (documento 02, §4) — no introduce una sintaxis de lambda aparte.
- Si se omite `catch` (como en la segunda línea, `try parse(text)`), el tipo de error de `expr` debe coincidir exactamente con el `E` de la función contenedora; si no coincide, error de compilación pidiendo un `catch` explícito.

## 3. Panics

Reservados para errores de programación (bugs), no para fallos esperables del dominio:

```ostrin
items = [1, 2, 3]
items[10]              // panic: index out of bounds (10, length 3)

a = 10
b = 0
a / b                  // panic: division by zero
```

- Un panic imprime un mensaje descriptivo (operación, valores involucrados, ubicación en el código) y termina el programa — o, dentro de un modelo de concurrencia con tareas aisladas (pendiente de diseñar), termina la tarea que lo produjo sin tumbar el proceso completo.
- Panics **no se capturan en línea** (no existe un `try/catch` de panics dentro del flujo normal). Esto es intencional: si algo es lo bastante grave como para hacer panic, tratar de "seguir como si nada" en el mismo punto suele enmascarar el bug en vez de arreglarlo.
- `panic(mensaje: String) -> Never` está disponible como función explícita para invariantes del programador:

```ostrin
fn set_temperature(t: Quantity<Temperature>) {
    if t < 0 K { panic("Temperature below absolute zero: " + t.to_string()) }
    ...
}
```

### Desempaquetado explícito (`unwrap`, `expect`)

Para los casos en que el programador sabe (o asume) que un `Result`/`Option` no va a fallar, y prefiere un panic claro a propagar el error hacia arriba:

```ostrin
config = load_config("app.toml").unwrap()             // panic genérico si es Err
config = load_config("app.toml").expect("config debe existir en producción")   // panic con mensaje propio
```

`unwrap()`/`expect()` son la puerta de escape explícita del sistema de errores — nunca implícitas, siempre visibles como una decisión consciente de "aquí prefiero un panic a manejar el error".

## 4. Métodos de combinación

```ostrin
Option<T>:
    .map(fn(T) -> U) -> Option<U>
    .then(fn(T) -> Option<U>) -> Option<U>      // encadena, evita Option<Option<U>>
    .unwrap_or(default: T) -> T
    .ok_or(error: E) -> Result<T, E>
    .is_some() -> Bool
    .is_none() -> Bool

Result<T, E>:
    .map(fn(T) -> U) -> Result<U, E>
    .map_err(fn(E) -> F) -> Result<T, F>
    .then(fn(T) -> Result<U, E>) -> Result<U, E>
    .unwrap_or(default: T) -> T
    .ok() -> Option<T>
    .is_ok() -> Bool
    .is_err() -> Bool
```

`.then(...)` cumple el rol que en otros lenguajes se llama `and_then`/`flatMap` — se eligió ese nombre por legibilidad ("intenta esto, *luego* esto otro").

## 5. Valores `Result`/`Option` no consumidos

Para que olvidar manejar un error no pase desapercibido: si una expresión de tipo `Result<T, E>` (o `Option<T>`) se evalúa como sentencia y su valor no se usa de ninguna forma (no se asigna, no se le aplica `try`, `.unwrap()`, `.map()`, etc.), el compilador emite una advertencia:

```text
Warning OSTRIN-W2001
Unused Result value.
'write_file(path, data)' returns Result<Void, String> which may be an error;
the failure is silently discarded.
Use 'try', '.unwrap()', or explicitly '_ = ...' to acknowledge it.
```

`_ = write_file(path, data)` descarta el resultado de forma explícita, dejando constancia en el código de que fue una decisión y no un olvido.

---

## 6. Preguntas abiertas para la siguiente sesión de diseño

1. ~~Panics dentro de concurrencia~~ — resuelto en el documento 09, §1 (`.join()` propaga el panic de la tarea).
2. ~~`enum` en general~~ — resuelto en el documento 05.
3. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
