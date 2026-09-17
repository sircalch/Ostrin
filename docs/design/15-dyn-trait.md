# Ostrin — Diseño core: `dyn Trait` (polimorfismo dinámico)

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [03-traits.md](03-traits.md), [11-modelo-de-memoria.md](11-modelo-de-memoria.md), [13-map-set-y-literales-de-coleccion.md](13-map-set-y-literales-de-coleccion.md)

Decisión de fondo ya cerrada:
- **`dyn Trait` existe, como mecanismo de escape explícito** — no reemplaza a los genéricos (que siguen siendo la forma por defecto, resuelta en compilación), se añade aparte para el único caso real que los genéricos y los `enum` no resuelven bien: una colección de tipos concretos distintos, potencialmente definidos en módulos o librerías diferentes, que comparten un trait pero no un `enum` cerrado que los una.

---

## 1. Por qué hace falta esto además de genéricos y `enum`

- Un genérico `<T: Shape>` resuelve "esta función funciona para cualquier tipo que implemente `Shape`" — pero **una sola llamada** siempre trabaja con **un solo `T` concreto** a la vez (el compilador genera una versión especializada del código por cada `T` usado, "monomorphization"). No sirve para "una lista que mezcla círculos, rectángulos y triángulos al mismo tiempo".
- Un `enum` (documento 05) sí modela "una de varias formas concretas" en una sola lista — pero exige que **todas** las variantes se conozcan y se declaren en un solo lugar. No sirve cuando una librería externa quiere añadir su propio tipo que implementa `Shape` sin tocar el `enum` de nadie (el caso típico de un sistema de plugins, o simplemente una librería de formas geométricas que otros extienden).
- `dyn Trait` cubre exactamente ese hueco: valores de tipos concretos distintos y potencialmente desconocidos en el momento de compilar la colección, unidos solo por implementar el mismo trait.

## 2. Sintaxis y uso

```ostrin
trait Shape {
    fn area(self) -> Quantity<Length^2>
}

record Circle { radius: Quantity<Length> }
record Rectangle { width: Quantity<Length>, height: Quantity<Length> }

impl Shape for Circle {
    fn area(self) -> Quantity<Length^2> { 3.14159 * self.radius * self.radius }
}
impl Shape for Rectangle {
    fn area(self) -> Quantity<Length^2> { self.width * self.height }
}

shapes: List<dyn Shape> = [Circle { radius: 2 m }, Rectangle { width: 3 m, height: 4 m }]

for shape in shapes {
    print(shape.area())
}
```

- `dyn Trait` es un tipo en sí mismo: "cualquier valor, sea cual sea su tipo concreto, que implemente `Trait`". Se usa donde iría cualquier otro tipo — como parámetro de función, tipo de elemento de `List`/`Map`/`Set`, tipo de retorno, campo de un `record`.
- **Asignar un valor concreto a una posición de tipo `dyn Trait` es implícito**, sin conversión explícita (`Circle { ... }` se acepta directamente donde se espera `dyn Shape`) — esto no es una excepción a "sin coerción implícita" (documento 01, §3.3): esa regla habla de coerciones que pierden o inventan información (un escalar convirtiéndose en una cantidad física con unidad arbitraria); aquí no se pierde ni se inventa nada, solo se "olvida" temporalmente el tipo concreto para tratarlo de forma uniforme — es la misma clase de operación que asignar cualquier valor a una variable de un tipo menos específico en cualquier lenguaje con subtipado de interfaces.
- Dentro de una función genérica sobre `T: Shape`, cada llamada a `shape.area()` se resuelve **en compilación**, sin costo extra (se sabe exactamente qué código ejecutar). Sobre un `dyn Shape`, la llamada se resuelve **en tiempo de ejecución** (una tabla de métodos por tipo, consultada en cada llamada) — el precio a pagar por la flexibilidad de mezclar tipos que el compilador no puede enumerar de antemano.

## 3. Traits "compatibles con `dyn`" (object safety)

No todos los traits pueden usarse como `dyn Trait`. Un trait es válido para esto solo si **todos** sus métodos cumplen:

1. **Ningún método tiene sus propios parámetros genéricos.** Un método `fn transform<U>(self, f: fn(T) -> U) -> U` no se puede despachar dinámicamente, porque haría falta generar una versión distinta por cada `U` posible en tiempo de ejecución — lo opuesto a lo que la resolución dinámica puede hacer.
2. **Ningún método devuelve `Self` por valor.** A través de un `dyn Trait` no se conoce el tipo concreto, así que no hay forma de nombrar "un `Self` nuevo" como tipo de retorno — sí se puede devolver `dyn Trait` (el mismo trait, sin comprometerse al tipo concreto), pero no `Self`.

```ostrin
trait Cloneable {
    fn duplicate(self) -> Self   // devuelve Self: NO es compatible con dyn
}
```

```text
Error OSTRIN-E1130
Trait 'Cloneable' cannot be used as 'dyn Cloneable': method 'duplicate' returns 'Self'.
A dyn-compatible trait's methods cannot return the concrete implementing type,
since it is erased at the point of dynamic dispatch.
```

Intentar escribir `dyn Cloneable` en cualquier posición de tipo, para un trait que no cumple estas reglas, es error de compilación en el sitio donde se usa `dyn Cloneable`, señalando cuál de las dos reglas rompe.

## 4. `mut self` a través de `dyn Trait`

La distinción `self`/`mut self` (documento 03, §1.1) se conserva sin cambios: llamar un método `mut self` de un `dyn Trait` exige que el binding que lo sostiene sea `mut`, exactamente igual que con cualquier otro valor:

```ostrin
mut shapes: List<dyn Resizable> = [...]
shapes.get(0).unwrap().resize(2.0)   // OK si 'resize' es 'mut self' y 'shapes' es mut
```

## 5. Representación en memoria

Un valor `dyn Trait` no tiene tamaño conocido en compilación (podría ser un `Circle` pequeño o un `record` grande con muchos campos) — igual que ocurre con los campos recursivos de un `enum` (documento 11, §5), el compilador lo asigna automáticamente en el heap. Concretamente, un `dyn Trait` se representa con dos punteros: uno al valor real (gestionado por ARC como cualquier otro valor con identidad, documento 11, §2) y otro a una tabla de métodos (una función por cada método del trait, apuntando a la implementación concreta de ese tipo). Esto es, de nuevo, invisible para el programador — coherente con la decisión de no exponer `Box<T>` explícito (documento 11): el compilador ya necesitaba resolver indirección automática para casos como este.

## 6. Combinando varios traits

Igual que un trait bound de un genérico admite varios traits con `+` (documento 03, §3), un `dyn Trait` también:

```ostrin
shapes: List<dyn Shape + Printable> = [...]
```

Cada valor de la lista debe implementar **ambos** traits — el compilador genera una tabla de métodos combinada por detrás; sigue sin ser visible ni configurable desde el código Ostrin.

---

## 7. Preguntas abiertas para la siguiente sesión de diseño

1. **`match` sobre `dyn Trait`** — no tiene sentido igual que sobre un `enum` (no hay un conjunto cerrado de "variantes"); si hace falta recuperar el tipo concreto desde un `dyn Trait` (downcasting), sería un mecanismo aparte, no diseñado aquí.
2. Pendientes previos siguen abiertos: `as D`, `select` sobre canales, cancelación de tareas, elisión de ARC, operadores bit a bit, ordenación general.
