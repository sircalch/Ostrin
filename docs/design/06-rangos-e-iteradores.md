# Ostrin — Diseño core: Rangos, Bucles e Iteradores

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [03-traits.md](03-traits.md), [05-enum-y-pattern-matching.md](05-enum-y-pattern-matching.md)

**Corrección sobre el documento 05**: el `1..10` usado de forma anticipada en §2.5 queda reemplazado por la sintaxis definida aquí (`1 to 10` / `0 until n`).

Decisiones de fondo ya cerradas:
- Rangos con **palabras legibles** (`to` / `until`), no símbolos (`..`, `..=`, `..<`).

---

## 1. Rangos

```ostrin
1 to 10        // 1, 2, 3, ..., 10   (incluye ambos extremos)
0 until 10     // 0, 1, 2, ..., 9    (excluye el límite superior)
```

- `to`: rango **inclusivo** en ambos extremos.
- `until`: rango **exclusivo** en el límite superior. Es el que se usa para "los primeros N elementos" (`0 until n`), evitando el clásico error de recontar `n-1` a mano.
- Un rango es un valor de tipo `Range<T>`, donde `T` debe implementar `Ord` (documento 03) — funciona para `Int`, y también para `Quantity<D>`:

```ostrin
300 K to 320 K
0 m until 10 m
```

- Un rango con paso distinto de 1 se declara con `step`:

```ostrin
0 until 100 step 5     // 0, 5, 10, ..., 95
10 to 0 step -1        // 10, 9, 8, ..., 0
```

### 1.1 Rangos y `within`

El operador de pertenencia a un rango, ya anticipado en la idea original de Ostrin, se apoya directamente en `Range<T>`:

```ostrin
if temperature within (300 K to 320 K) {
    ...
}
```

`within` es azúcar sintáctico sobre un método `.contains(value)` de `Range<T>` — `a within r` desazucara a `r.contains(a)`. Se documenta junto con el resto de operadores de comparación cuando se cierre el sistema de expresiones/lógica (pendiente ya anotado en documentos previos).

## 2. Bucles

### 2.1 `for` — iterar sobre un rango o colección

```ostrin
for i in 1 to 10 {
    print(i)
}

for item in items {
    print(item)
}
```

- La variable de iteración (`i`, `item`) es un binding **inmutable** nuevo en cada vuelta (consistente con la regla general de variables del documento 01) — no se puede reasignar dentro del cuerpo, aunque sí se puede hacer shadowing normal si hace falta transformarla localmente.
- `for` es una sentencia, no una expresión: siempre "vale" `Void`. Para producir un valor a partir de una iteración se usa `map`/`fold` sobre la colección (ver §4), no un `for`.

### 2.2 `while` — condición evaluada antes de cada vuelta

```ostrin
mut remaining = 100
while remaining > 0 {
    remaining = remaining - 1
}
```

Igual que `for`: sentencia, siempre `Void`.

### 2.3 `loop` — bucle infinito, expresión con valor vía `break`

```ostrin
result = loop {
    attempt = try_connect()
    if attempt.is_ok() {
        break attempt.unwrap()
    }
}
```

- `loop { ... }` repite indefinidamente hasta un `break`.
- A diferencia de `for`/`while`, `loop` **es una expresión**: `break valor` termina el bucle y ese `valor` es el resultado de la expresión `loop` completa (consistente con que `if`/`match` también son expresiones). Un `break` sin valor dentro de un `loop` usado como expresión es error de compilación si el tipo de resultado esperado no es `Void`.
- `continue` salta a la siguiente vuelta en cualquiera de los tres bucles (`for`, `while`, `loop`).

## 3. El protocolo de iteración

Para que `for` funcione sobre cualquier tipo del usuario (no solo `List`/`Range` incorporados), Ostrin define el protocolo vía traits (documento 03), igual que el resto de comportamiento extensible del lenguaje:

```ostrin
trait Iterator<T> {
    fn next(self) -> Option<T>
}

trait Iterable<T> {
    fn iterator(self) -> Iterator<T>
}
```

- Un `for item in coleccion { ... }` desazucara a: obtener `coleccion.iterator()`, y llamar `.next()` repetidamente mientras devuelva `Some(item)`, hasta el primer `None`.
- `next(mut self)` necesita mutar el estado interno del iterador (la posición actual) en cada llamada — por eso se declara `mut self` (documento 03, §1.1), no `self` a secas.
- **Por qué el llamador del `for` no necesita declarar nada como `mut`**: el objeto que el `for` obtiene de `.iterator()` es un valor **recién creado**, exclusivo del propio `for`, que nadie más referencia todavía. La regla general de "llamar un método `mut self` exige tenerlo en mano de forma mutable" (documento 03, §1.1) incluye exactamente este caso — un valor recién construido sin otro dueño se puede tratar como mutable aunque la colección original de la que salió no lo sea. La mutabilidad del iterador es un detalle interno de cómo se recorre, no algo que se filtre hacia el binding de la colección original.
- Un tipo puede implementar `Iterator<T>` **directamente sobre sí mismo** (en vez de a través de un `Iterable<T>` separado) cuando la propia secuencia es, por naturaleza, un objeto de un solo paso con estado — es el caso de `Fibonacci` abajo: no hace falta un `.iterator()` que devuelva *otro* objeto, porque `Fibonacci` ya *es* su propio iterador.
- Cualquier tipo de usuario puede volverse iterable implementando `Iterable<T>` (o `Iterator<T>` directamente, como aquí):

```ostrin
record Fibonacci {
    mut current: Int
    mut next_value: Int
}

impl Iterator<Int> for Fibonacci {
    fn next(mut self) -> Option<Int> {
        value = self.current
        self.current = self.next_value
        self.next_value = value + self.next_value
        Some(value)
    }
}

for n in fibonacci_up_to(100) {
    print(n)
}
```

## 4. Operaciones funcionales sobre colecciones/iteradores

Para producir un valor a partir de recorrer algo (en vez de un `for` con efectos secundarios), la stdlib provee las operaciones estándar sobre cualquier `Iterable<T>`:

```ostrin
List<T>:
    .map(fn(T) -> U) -> List<U>
    .filter(fn(T) -> Bool) -> List<T>
    .fold(initial: U, fn(U, T) -> U) -> U
    .find(fn(T) -> Bool) -> Option<T>
    .any(fn(T) -> Bool) -> Bool
    .all(fn(T) -> Bool) -> Bool
    .count() -> Int
```

```ostrin
total_mass = particles
    .filter(fn(p) { p.charge != 0 C })
    .map(fn(p) { p.mass })
    .fold(0 kg, fn(acc, m) { acc + m })
```

Estas operaciones están definidas una sola vez sobre `Iterable<T>` en la stdlib y funcionan automáticamente para `List`, `Range`, y cualquier tipo de usuario que implemente `Iterable` — no hay que reimplementarlas por tipo.

---

## 5. Preguntas abiertas para la siguiente sesión de diseño

1. ~~Sistema de expresiones/lógica completo~~ — resuelto en el documento 08.
2. ~~Colecciones incorporadas más allá de `List`~~ — resuelto en el documento 13 (`Map`, `Set`, literales de colección).
3. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
