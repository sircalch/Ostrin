# Ostrin — Diseño core: Traits

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md)

Decisiones de fondo ya cerradas:
- Implementación **nominal explícita** (`impl Trait for Type`), no estructural.
- Traits pueden incluir **métodos con implementación por defecto**.
- Operadores (`+`, `-`, `==`, `<`, ...) se sobrecargan **implementando traits estándar**, igual para tipos de usuario que para `Quantity<D>`.

---

## 1. Declaración de un trait

```ostrin
trait Comparable {
    fn equals(self, other: Self) -> Bool
    fn not_equals(self, other: Self) -> Bool {
        !self.equals(other)
    }
}
```

- `Self` dentro de un trait se refiere al tipo concreto que lo implementa (se resuelve en cada `impl`).
- `self` como primer parámetro de un método es la **única excepción** a la regla de "todo parámetro requiere tipo explícito" (documento 02, §1): su tipo es siempre `Self` de forma implícita, nunca se escribe `self: Self`. Es una excepción reconocida, no un descuido — escribir el tipo de `self` en cada método de cada `impl` sería ruido puro, porque solo puede ser un tipo.
- Un método sin cuerpo (`fn equals(self, other: Self) -> Bool`) es **obligatorio**: cualquier `impl` debe proveerlo.
- Un método con cuerpo (`fn not_equals(...) { ... }`) es **default**: el `impl` puede omitirlo y hereda ese comportamiento, o sobrescribirlo si necesita una versión más eficiente.

### 1.1 `self` de solo lectura vs `mut self`

El receptor de un método se declara de dos formas distintas, según si el método necesita reasignar algún campo `mut` del propio valor:

```ostrin
trait Iterator<T> {
    fn next(mut self) -> Option<T>   // muta campos de self: se declara 'mut self'
}

trait Comparable {
    fn equals(self, other: Self) -> Bool   // no muta nada: 'self' a secas
}
```

- `fn metodo(self, ...)`: receptor de **solo lectura**. El cuerpo puede leer los campos de `self`, pero reasignar cualquiera de ellos (incluso uno declarado `mut` en el `record`) es error de compilación.
- `fn metodo(mut self, ...)`: receptor **mutable**. El cuerpo puede reasignar los campos que el `record`/`enum` declaró `mut` (documento 01, §1.2 — un campo sin `mut` sigue siendo inmutable aunque el método reciba `mut self`).
- **Llamar un método `mut self` exige tener el valor "en mano" de forma mutable**: o bien a través de un binding `mut` (`mut fib = Fibonacci { ... }; fib.next()`), o bien desde dentro de otro método `mut self` del mismo valor (la mutabilidad se propaga hacia dentro, no hace falta re-declararla en cada llamada interna), o bien sobre un valor recién construido que todavía no tiene ningún otro binding apuntándolo (por ejemplo, el iterador temporal que un `for` obtiene de `.iterator()` — ver documento 06, §3, que depende exactamente de esta regla).
- Intentar llamar un método `mut self` sobre un binding inmutable es el mismo tipo de error que reasignar directamente un binding inmutable (documento 01, §1.1):

```text
Error OSTRIN-E1053
Cannot call 'next' (requires 'mut self') on immutable binding 'fib'.
Declare it as 'mut fib = ...' to allow calling mutating methods.
```

## 2. Implementación — `impl Trait for Type`

```ostrin
record Particle {
    mass: Quantity<Mass>
    charge: Quantity<Charge>
}

impl Comparable for Particle {
    fn equals(self, other: Self) -> Bool {
        self.mass == other.mass and self.charge == other.charge
    }
}
```

- `Particle` satisface `Comparable` **solo** porque existe este bloque `impl`, aunque `Particle` ya tuviera un método `equals` con esa firma exacta antes del `impl` — la firma nunca es suficiente por sí sola (tipado nominal, no estructural).
- Un tipo puede tener múltiples `impl` de traits distintos, pero **como máximo un `impl` de un trait dado por tipo** (regla de coherencia: no puede haber dos implementaciones de `Comparable for Particle` compitiendo — error de compilación si se detecta).
- El `impl` puede vivir en el mismo archivo que el `record`, o en otro módulo — no es obligatorio implementarlo junto a la declaración del tipo. Esto permite implementar traits propios sobre tipos de la stdlib (ver §5).

## 3. Traits como límites de genéricos (`trait bounds`)

Esto es lo que hace que `<T: Comparable>` mencionado en el documento de funciones tenga sentido real:

```ostrin
fn max<T: Comparable>(a: T, b: T) -> T {
    if a.equals(b) or a > b { a } else { b }
}
```

- `<T: Comparable>` restringe `T` a "cualquier tipo que tenga un `impl Comparable for T`". Dentro del cuerpo, el compilador permite llamar los métodos de `Comparable` sobre valores de tipo `T`, y nada más (no se puede asumir ningún otro método que `T` no garantice por trait).
- Múltiples bounds se combinan con `+`:

```ostrin
fn describe<T: Comparable + Printable>(x: T) -> String {
    ...
}
```

- Sin bound (`<T>` a secas), el genérico solo permite operaciones válidas para *cualquier* tipo (asignar, pasar como parámetro, devolver) — nada que dependa de un método específico.

## 4. Operadores vía traits estándar

La stdlib define un trait por cada operador sobrecargable. Implementarlo habilita la sintaxis del operador:

```ostrin
trait Add {
    fn add(self, other: Self) -> Self
}

trait Eq {
    fn equals(self, other: Self) -> Bool
}

trait Ord {
    fn compare(self, other: Self) -> Ordering   // Ordering = Less | Equal | Greater
}
```

```ostrin
record Vector2 {
    x: Float
    y: Float
}

impl Add for Vector2 {
    fn add(self, other: Self) -> Self {
        Vector2 { x: self.x + other.x, y: self.y + other.y }
    }
}

v1 + v2   // desazucara a v1.add(v2), válido porque Vector2 implementa Add
```

- `v1 + v2` es azúcar sintáctico que el compilador reescribe a `v1.add(v2)` **solo si** el tipo de `v1` implementa `Add`. Si no, es error de compilación ("`Vector2` no implementa `Add`, el operador `+` no está definido para este tipo"), nunca un intento de coerción implícita.
- **`Quantity<D>` no es un caso especial "mágico" separado**: sus operadores (`+`, `-`, `*`, `/`, `==`, `<`, etc., con las reglas de dimensión ya definidas en el documento 01) están implementados por la stdlib exactamente vía estos mismos traits (`impl<D: Dimension> Add for Quantity<D>`, etc.). Esto unifica el modelo: no hay un mecanismo de operadores para "tipos built-in" y otro distinto para tipos de usuario.
- Comparación de igualdad estructural por defecto: los `record` **no** implementan `Eq` automáticamente — hay que declararlo (`impl Eq for Particle { ... }`) o generarlo automáticamente con `record Particle: Eq { ... }` (mecanismo `derive`, formalizado en el documento 12).

## 5. Implementar traits sobre tipos ajenos (incluida la stdlib)

```ostrin
impl Printable for Quantity<Length> {
    fn to_display(self) -> String {
        ...
    }
}
```

Permitido, con la **regla de coherencia** (equivalente a la "orphan rule" de Rust) para evitar ambigüedad global: un `impl Trait for Type` solo es válido si el trait **o** el tipo fueron definidos en el módulo actual (o uno importado explícitamente que lo autorice). No se puede, desde un módulo externo cualquiera, implementar un trait de la stdlib para un tipo de la stdlib — eso evita que dos librerías distintas definan implementaciones incompatibles del mismo trait para el mismo tipo sin que el programa lo detecte.

## 6. Traits y `Dimension`

Un caso particular relevante para Ostrin: un `impl` puede ser genérico sobre la dimensión, cubriendo **todas** las cantidades físicas de una sola vez:

```ostrin
impl<D: Dimension> Comparable for Quantity<D> {
    fn equals(self, other: Self) -> Bool {
        // ya garantizado por el sistema de tipos que other es la misma D
        ...
    }
}
```

Esto es lo que permite que `average<D: Dimension>(values: List<Quantity<D>>)` (documento 02) pueda, por ejemplo, pedir también `<D: Dimension> where Quantity<D>: Comparable` sin tener que escribir un `impl` distinto por cada dimensión concreta (Length, Time, Mass, ...).

---

## 7. Preguntas abiertas para la siguiente sesión de diseño

1. ~~`derive`~~ y ~~herencia entre traits~~ — resueltos en el documento 12.
2. ~~Traits como tipos existenciales (`dyn Trait`)~~ — resuelto en el documento 15.
3. ~~`as D`~~ — resuelto en el documento 16 (no hacía falta ningún mecanismo nuevo).
