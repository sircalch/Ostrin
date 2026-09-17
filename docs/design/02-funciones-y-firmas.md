# Ostrin — Diseño core: Funciones y Firmas

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md)

Decisiones de fondo ya cerradas:
- Tipo de retorno **explícito obligatorio** en la firma.
- **Sin distinción pure/impure** en el sistema de tipos (se puede añadir después sin romper esto).
- Soporta **argumentos con nombre y valores por defecto**.

---

## 1. Declaración básica

```ostrin
fn velocity(distance: Quantity<Length>, time: Quantity<Time>) -> Quantity<Length / Time> {
    distance / time
}
```

- `fn nombre(params) -> TipoRetorno { cuerpo }`.
- El cuerpo es una secuencia de expresiones; **la última expresión es el valor de retorno** (estilo expresión, como Rust), sin necesidad de `return` en el caso normal.
- `return` explícito solo se usa para salir anticipadamente (dentro de un `if` en medio del cuerpo, por ejemplo).

```ostrin
fn clamp(x: Float, low: Float, high: Float) -> Float {
    if x < low { return low }
    if x > high { return high }
    x
}
```

- Todo parámetro requiere tipo explícito (igual que el retorno). No hay inferencia de tipos de parámetros: la firma es el contrato completo, se lee sin mirar el cuerpo.

## 2. Argumentos con nombre y valores por defecto

```ostrin
fn simulate(temperature: Quantity<Temperature>, pressure: Quantity<Pressure>, steps: Int = 100) -> Result<Trajectory, String> {
    ...
}
```

Llamada:

```ostrin
simulate(temperature: 310 K, pressure: 1 atm)
simulate(temperature: 310 K, pressure: 1 atm, steps: 500)
simulate(pressure: 1 atm, temperature: 310 K)   // orden libre si se nombran todos
```

Reglas:
- Todo parámetro tiene un nombre; ese nombre **es** el argumento con el que se llama (no hay `_` para "sin etiqueta" como en Swift — en Ostrin siempre se nombra, por consistencia y legibilidad en llamadas científicas).
- Se puede llamar posicionalmente si se respeta el orden de la firma: `simulate(310 K, 1 atm)` es válido y equivalente a nombrar ambos. Mezclar es válido siempre que los posicionales vengan primero: `simulate(310 K, pressure: 1 atm)`.
- Un parámetro con valor por defecto puede omitirse en la llamada; los parámetros sin valor por defecto no pueden ir después de uno que sí lo tiene, salvo que se sigan llamando por nombre.
- El valor por defecto se evalúa en el contexto de definición de la función, no en el de la llamada (evita sorpresas con variables capturadas mutables).

## 3. Genéricos

Ostrin distingue dos tipos de parámetro genérico, porque una función puede necesitar generalizar sobre **tipo** o sobre **dimensión física**, y son cosas distintas:

### 3.1 Genérico de tipo — `<T>`

```ostrin
fn first<T>(items: List<T>) -> Option<T> {
    ...
}
```

Igual que en cualquier lenguaje con genéricos: `T` puede ser cualquier tipo (`Int`, `String`, un `record`, etc.).

### 3.2 Genérico de dimensión — `<D: Dimension>`

Esta es la pieza que no existe en la mayoría de lenguajes y es central para Ostrin: una función que opera sobre **cualquier cantidad física, sea cual sea su dimensión**, sin perder la verificación de tipos.

```ostrin
fn double<D: Dimension>(x: Quantity<D>) -> Quantity<D> {
    x * 2
}

double(5 nm)      // -> 10 nm       (D = Length)
double(3 s)       // -> 6 s         (D = Time)
double(12)        // Error: 12 es Int, no Quantity<D>
```

- `<D: Dimension>` declara que `D` es una variable de dimensión, no un tipo concreto. El compilador la resuelve en cada sitio de llamada según el argumento recibido, igual que resolvería `T` en un genérico normal — pero en vez de unificar tipos, unifica **vectores de exponentes**.
- Esto permite escribir, por ejemplo, una función de conversión de unidades genérica:

```ostrin
fn convert<D: Dimension>(x: Quantity<D>, target: Unit<D>) -> Quantity<D> {
    ...
}

convert(5 nm, target: km)     // OK, misma dimensión (Length)
convert(5 nm, target: kg)     // Error de compilación: kg no es de dimensión Length
```

`Unit<D>` es el tipo de "una unidad concreta de la dimensión D" (el propio símbolo `km`, `nm`, etc., no un valor con esa unidad) — se usa para pasar unidades como parámetro, como en `convert` arriba.

### 3.3 Combinando ambos

```ostrin
fn average<D: Dimension>(values: List<Quantity<D>>) -> Quantity<D> {
    sum(values) / values.length()
}
```

`values.length()` es `Int` (un escalar puro, sin dimensión); dividir una `Quantity<D>` entre un escalar conserva la dimensión `D` (documento 01, §3.3) — no hace falta ninguna conversión ni construcción especial. (Una versión anterior de este ejemplo usaba `values.length() as D`, asumiendo que hacía falta convertir el conteo a una `Quantity<D>` antes de dividir — además de innecesario, habría sido semánticamente incorrecto: dividir `Quantity<D> / Quantity<D>` cancela la dimensión a `Float` puro, doc01 §3.3, lo cual habría perdido la unidad del promedio. Corregido en el documento 16.)

## 4. Funciones como valores / closures

Las funciones son ciudadanos de primera clase:

```ostrin
square: fn(Float) -> Float = fn(x) { x * x }

apply_twice = fn(f: fn(Float) -> Float, x: Float) -> Float {
    f(f(x))
}

apply_twice(square, 3.0)   // 81.0
```

- El tipo de una función es `fn(TiposParametros) -> TipoRetorno`.
- Una función anónima (lambda) se escribe `fn(params) { cuerpo }`, sin nombre, y puede asignarse a un binding como cualquier otro valor (siguiendo las mismas reglas de mutabilidad de §1 del documento anterior). Es la **única** sintaxis de lambda en Ostrin — no existe una segunda forma abreviada (p. ej. `|params| { ... }`); cualquier lugar del lenguaje que necesite un closure lo escribe así, sin excepción, para no obligar a aprender dos sintaxis distintas para la misma cosa.
- **Closures y captura**: una función anónima puede capturar bindings del scope donde se define. Capturar un binding inmutable captura su *valor* en ese punto, sin sorpresas de aliasing. Capturar un binding `mut` **sí está permitido dentro de la misma tarea de ejecución** (un closure normal, usado en el mismo hilo de control que lo definió — p. ej. pasado a `.map()`, `.filter()`, o guardado y llamado más adelante en el mismo flujo secuencial) y puede leer y reasignar ese `mut` con normalidad, como una referencia compartida al mismo binding. **Esto es distinto y más restrictivo cuando el closure cruza un límite de concurrencia** (`spawn`): ahí, capturar un `mut` está prohibido por completo — ver documento 09 (Concurrencia), §1.1. La diferencia no es "pureza" (Ostrin no rastrea pureza, ver decisión de fondo de este documento) sino, simplemente, si el closure puede ejecutarse en paralelo con quien lo creó o no.

### 4.1 Bloques finales (`trailing closures`)

Cuando el **último parámetro** de una función es de tipo `fn(...) -> T`, se puede pasar como un bloque `{ }` después de la lista de argumentos, en vez de escribirlo como argumento normal:

```ostrin
fn with_file<T>(path: String, body: fn(File) -> T) -> T {
    file = open(path)
    result = body(file)
    file.close()
    result
}

with_file("data.csv") { file ->
    process(file)
}

// Exactamente equivalente a:
with_file("data.csv", fn(file) { process(file) })
```

- `nombre(argumentos) { param -> cuerpo }` desazucara a `nombre(argumentos, fn(param) { cuerpo })`. Si el closure no toma parámetros, se omite la parte `param ->`.
- Esta es una **regla general del lenguaje**, no un caso especial de `loop`/`spawn`/`spawn_scope` (documentos 06 y 09): cualquier función de la stdlib o del código del usuario cuyo último parámetro sea una función puede llamarse con esta forma. `loop { ... }`, `spawn { ... }` y `spawn_scope { ... }` son, de hecho, simplemente funciones (o construcciones del lenguaje con la misma forma) que aprovechan esta misma regla, no una sintaxis aparte.
- Solo se permite un bloque final por llamada (el último parámetro función); si una función tiene dos parámetros de tipo función, solo el último puede escribirse como bloque, los demás se pasan de forma explícita con `fn(...) { ... }`.

## 5. Sobrecarga (overloading)

Ostrin **no permite sobrecarga por nombre** (dos `fn` distintas con el mismo nombre y distinta firma en el mismo scope es error de compilación), con una única excepción ya cubierta por el diseño: la genericidad de dimensión (`<D: Dimension>`) ya resuelve el caso más común que en otros lenguajes forzaría sobrecarga (una función que "funciona para cualquier unidad").

```ostrin
fn area(side: Quantity<Length>) -> Quantity<Length^2> { side * side }
fn area(radius: Quantity<Length>, pi_approx: Bool) -> Quantity<Length^2> { ... }
```
```text
Error OSTRIN-E1040
Function 'area' is already defined with a different signature at line 1.
Ostrin does not support overloading by name — consider distinct names
(e.g. 'area_square', 'area_circle') or a single signature using Option/Result.
```

Motivo: la resolución de sobrecarga es una fuente clásica de ambigüedad y errores difíciles de leer (especialmente combinada con conversión automática de unidades); preferimos nombres explícitos.

## 6. Recursión

Recursión directa e indirecta permitidas sin sintaxis especial (una `fn` puede llamarse a sí misma o a otra `fn` declarada después en el mismo módulo — no se exige "declarar antes de usar" dentro de un mismo módulo).

```ostrin
fn factorial(n: Int) -> Int {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}
```

---

## 7. Preguntas abiertas para la siguiente sesión de diseño

1. **`as D` y conversión genérica de escalares a cantidades** (usado en el ejemplo de `average`) — necesita su propio diseño detallado como parte del sistema de conversiones. Sigue abierto.
2. ~~Reglas exactas de captura de `mut` en closures~~ — resuelto en §4.
3. ~~Traits/interfaces~~ — resuelto en el documento 03.
4. ~~Manejo de errores y `Result`~~ — resuelto en el documento 04.
5. ~~Módulos y visibilidad~~ — resuelto en el documento 07.
