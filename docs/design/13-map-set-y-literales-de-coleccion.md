# Ostrin — Diseño core: `Map`, `Set` y Literales de Colección

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [03-traits.md](03-traits.md), [06-rangos-e-iteradores.md](06-rangos-e-iteradores.md), [11-modelo-de-memoria.md](11-modelo-de-memoria.md), [12-derive-y-herencia-de-traits.md](12-derive-y-herencia-de-traits.md)

Este documento cierra dos pendientes que venían arrastrándose desde el documento 06: la sintaxis literal de colecciones y los tipos `Map`/`Set` en sí. Las decisiones aquí son mayormente de gramática (cómo evitar ambigüedad con bloques `{ }` y con literales de `record`), no bifurcaciones filosóficas nuevas.

---

## 1. Por qué `{ }` no puede ser la sintaxis de colección

Antes de elegir la sintaxis, vale la pena decir por qué no se reutiliza `{ }` a secas (como Python hace con `{1, 2, 3}` para `set` y `{"a": 1}` para `dict`): en Ostrin, `{ }` **ya tiene un significado fijo** — es un bloque (cuerpo de función, `if`, `match`, closure), y un bloque vacío `{}` ya significa "no hacer nada, valor `Void`" (usado, por ejemplo, en el azúcar de `if let` del documento 05, §2.6). Si `{}` también significara "`Set` vacío" o "`Map` vacío", el caso vacío sería ambiguo sin más contexto — exactamente el tipo de ambigüedad que el resto del diseño ha evitado deliberadamente. Por eso Ostrin separa la sintaxis por familia de corchete y dejafuera el caso vacío de la sintaxis literal cuando haría falta reutilizar `{ }`.

## 2. `List<T>`

```ostrin
items = [1, 2, 3]          // List<Int>
empty: List<Int> = []
```

- `[elemento, elemento, ...]` construye un `List<T>`, donde `T` se infiere del tipo común de los elementos (error de compilación si no hay un tipo común — no hay coerción implícita entre tipos, documento 01).
- `[]` (vacío) es válido porque `[ ]` no colisiona con nada más en la gramática — a diferencia de `{ }`, los corchectes no se usan para bloques. Eso sí, un `[]` vacío casi siempre necesita anotación de tipo explícita al lado (`empty: List<Int> = []`), porque no hay elementos de los que inferir `T`.

### 2.1 Mutación de colecciones

Igual que cualquier binding (documento 01), un `List<T>` sin `mut` es de solo lectura — no se puede agregar ni quitar elementos, aunque se puedan leer e iterar libremente. Con `mut`:

```ostrin
mut items = [1, 2, 3]
items.push(4)              // OK, items es mut
removed = items.remove_at(0) // devuelve el elemento retirado
```

```ostrin
items = [1, 2, 3]
items.push(4)               // Error: 'items' es inmutable
```

La API de `List<T>` queda así:

```text
List<T>:
    .push(value: T) -> Void
    .remove_at(index: Int) -> T
```

`remove_at` produce un error de ejecución si el índice es negativo o está fuera
de rango; la operación no modifica la lista cuando falla.
```text
Error OSTRIN-E1053
Cannot call 'push' (requires 'mut self') on immutable binding 'items'.
Declare it as 'mut items = ...' to allow calling mutating methods.
```

Esto coloca a `List<T>` (y a `Map`/`Set`, más abajo) dentro de la misma regla del documento 11: un `List` con contenido mutable tiene identidad de referencia (dos bindings `mut` que apuntan al mismo `List` ven los cambios del otro), mientras que un `List` inmutable se trata por valor. Concretamente, `.push`/`.remove_at` (y, más abajo, `.set`/`.remove` de `Map`, `.add`/`.remove` de `Set`) son métodos declarados **`mut self`** (documento 03, §1.1) en la stdlib — por eso exigen un binding `mut` para poder llamarse: es la misma regla general de mutación de todo el lenguaje, no un caso especial de las colecciones.

## 3. `Map<K, V>`

```ostrin
prices = ["apple": 1.5, "bread": 2.0]      // Map<String, Float>
empty: Map<String, Int> = Map<String, Int>()
```

- `[clave: valor, clave: valor, ...]` construye un `Map<K, V>` — misma familia de corchete que `List`, diferenciado por la presencia de `:` entre cada par. El parser distingue ambos casos por esa marca, sin ambigüedad (una lista nunca tiene `:` a nivel superior entre sus elementos).
- El caso vacío **no** tiene forma literal (evita inventar un token especial tipo `[:]`): se construye con el constructor explícito `Map<K, V>()`, siempre con los tipos anotados porque no hay elementos de los que inferirlos.
- Requiere que `K` implemente `Hash + Eq` (ver §5). `V` no tiene restricción.

La primera implementación operativa usa una representación híbrida: las claves escalares de
`Map` y los elementos escalares de `Set` tienen índice hash con direccionamiento abierto y
mantienen las entradas en orden de inserción para que `keys()`/`values()` y la iteración sean
deterministas; las claves/elementos compuestos usan temporalmente el fallback lineal hasta que el
checker haga cumplir y el compilador genere `Hash` para tipos de usuario.

### 3.1 Operaciones

```ostrin
Map<K, V>:
    .get(key: K) -> Option<V>
    .contains_key(key: K) -> Bool
    .keys() -> List<K>
    .values() -> List<V>
    .count() -> Int

mut Map<K, V> además:
    .set(key: K, value: V) -> Void      // inserta o sobrescribe
    .remove(key: K) -> Option<V>        // quita y devuelve el valor si existía
```

- El orden de iteración de un `Map` **no está garantizado** — no se debe asumir ningún orden en particular (ni de inserción, ni alfabético). Si se necesita un orden específico, se ordena explícitamente (`map.keys().sorted()`, pendiente de la stdlib de ordenación general).
- Iterar un `Map` dvuelve pares `(clave, valor)` como `Tuple(K, V)`, y la variable de un `for` acepta directamente un patrón de desestructuración, no solo un identificador simple — generalización menor del documento 06, §2.1:

```ostrin
for (key, value) in prices {
    print(key + ": " + value.to_string())
}
```

## 4. `Set<T>`

```ostrin
seen = {1, 2, 3}                    // Set<Int>
empty = Set<Int>()                  // el caso vacío no tiene forma literal, mismo motivo que Map
```

- `{elemento, elemento, ...}` (con llaves, sin `:`) construye un `Set<T>`. Es sintácticamente distinguible de un bloque porque un bloque nunca es una lista de expresiones separadas por comas al nivel superior (las sentencias de un bloque se separan por salto de línea, no por coma) — el parser reconoce `{expr, expr, ...}` como literal de `Set`, no como bloque, en cuanto ve la primera coma a ese nivel.
- El caso vacío `{}` **sigue significando bloque vacío** (ya establecido, documento 05) — por eso un `Set<T>` vacío exige el constructor explícito `Set<T>()`, sin excepción.
- Requiere que `T` implemente `Hash + Eq` (§5).

### 4.1 Operaciones

```ostrin
Set<T>:
    .contains(x: T) -> Bool
    .count() -> Int

mut Set<T> además:
    .add(x: T) -> Void
    .remove(x: T) -> Void
```

Operadores de conjuntos, sobre cualquier `Set<T>` (mutable o no — producen un `Set` nuevo, no mutan los operandos):

```ostrin
a | b     // unión
a & b     // intersección
a - b     // diferencia
```

(Estos símbolos no colisionan con nada existente: Ostrin no tiene todavía operadores bit a bit sobre `Int` definidos — cuando se diseñen, deberán evitar reusar `|`/`&` para no chocar con esta notación de conjuntos; queda anotado como pendiente en §6.)

## 5. `trait Hash`

Necesario para que un tipo sea válido como clave de `Map` o elemento de `Set`:

```ostrin
trait Hash {
    fn hash(self) -> Int
}
```

- Se añade a la lista de traits derivables del documento 12: `derive(Hash)` combina el hash de todos los campos (misma filosofía que `derive(Eq)`/`derive(Ord)`: recorre los campos en orden de declaración, cada uno debe implementar `Hash` a su vez).
- `Map<K, V>` y `Set<T>` exigen `K`/`T`: `Hash + Eq` como trait bound — si se intenta usar un tipo que no los implementa, error de compilación señalando cuál de los dos falta, igual que cualquier otro trait bound (documento 03, §3).

```ostrin
record Point: Eq, Hash {
    x: Int
    y: Int
}

visited: Set<Point> = Set<Point>()
```

---

## 6. Preguntas abiertas para la siguiente sesión de diseño

1. **Operadores bit a bit para `Int`** — no diseñados todavía; deberán elegir símbolos/palabras que no choquen con `|`/`&`/`-` de conjuntos (candidato natural: palabras, `bitand`/`bitor`/`bitxor`, consistente con la preferencia general del lenguaje por palabras en operadores no aritméticos estándar).
2. **Ordenación general** (`.sorted()`, comparadores custom) — se mencionó de pasada en §3.1 pero no se ha diseñado formalmente.
3. ~~`dyn Trait`~~ — resuelto en el documento 15. Pendientes previos siguen abiertos: `select` sobre canales, elisión de ARC.
