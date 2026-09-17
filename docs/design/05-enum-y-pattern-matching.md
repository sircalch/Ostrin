# Ostrin — Diseño core: Enum y Pattern Matching

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [03-traits.md](03-traits.md), [04-errores-y-result.md](04-errores-y-result.md)

**Corrección sobre el documento 04**: los constructores de variante se escriben en `PascalCase`, sin excepción. `Option` es `Some(T)` / `None` (no `some(x)` / `none` como se escribió por descuido en los documentos 01 y 04). `Result` ya estaba bien: `Ok(T)` / `Err(E)`. Motivo: consistencia — un constructor de variante es, en la práctica, un valor con nombre de tipo, y todos los tipos/constructores en Ostrin van en `PascalCase`; las variables y funciones van en `snake_case`.

---

## 1. Declaración de `enum`

### 1.1 Variantes sin datos (como un enum clásico)

```ostrin
enum Ordering {
    Less
    Equal
    Greater
}
```

### 1.2 Variantes con datos asociados

```ostrin
enum Shape {
    Circle(radius: Quantity<Length>)
    Rectangle(width: Quantity<Length>, height: Quantity<Length>)
    Point
}
```

- Cada variante puede llevar cero o más campos. Los campos pueden ser posicionales (`Circle(Quantity<Length>)`) o nombrados (`Circle(radius: Quantity<Length>)`) — nombrarlos es la forma recomendada cuando hay más de un campo, por la misma razón que los parámetros de función se nombran (documento 02).
- Un `enum` es un tipo nuevo y distinto de cualquiera de sus variantes por separado; `Shape` es el tipo, `Circle`/`Rectangle`/`Point` son formas de construir un `Shape`.

### 1.3 Enums genéricos

```ostrin
enum Option<T> {
    Some(T)
    None
}

enum Result<T, E> {
    Ok(T)
    Err(E)
}

enum Tree<T> {
    Leaf
    Node(value: T, left: Tree<T>, right: Tree<T>)
}
```

`Tree<T>` muestra el caso recursivo: una variante puede referirse al propio `enum` que está definiendo (necesario para listas enlazadas, árboles, etc.).

### 1.4 Métodos sobre `enum`

Un `enum` puede tener bloques `impl` igual que un `record` (documento 03):

```ostrin
impl Shape {
    fn area(self) -> Quantity<Length^2> {
        match self {
            Circle(radius) => 3.14159 * radius * radius,
            Rectangle(width, height) => width * height,
            Point => 0 m^2,
        }
    }
}
```

## 2. `match`

### 2.1 Forma básica

```ostrin
description = match shape {
    Circle(radius) => "círculo de radio " + radius.to_string(),
    Rectangle(width, height) => "rectángulo " + width.to_string() + " x " + height.to_string(),
    Point => "punto",
}
```

- `match` es una **expresión**, no una sentencia: produce un valor, igual que `if` (documento 02). Todas las ramas deben devolver el mismo tipo.
- Cada rama es `patrón => expresión` (o `patrón => { bloque }` si necesita más de una línea), separadas por coma.

### 2.2 Exhaustividad obligatoria

El compilador exige que **todas** las variantes posibles estén cubiertas. Si falta una, es error de compilación, no un caso que se descubre en producción:

```ostrin
match shape {
    Circle(radius) => ...,
    Rectangle(width, height) => ...,
}
```
```text
Error OSTRIN-E1060
Non-exhaustive match on 'Shape'.
Missing variant: 'Point'.
Add a case for 'Point', or use '_' to cover remaining variants explicitly.
```

Esto es consecuencia directa de la decisión ya tomada de tipado estático estricto: un `match` que no cubre todo es exactamente el tipo de "error descubierto tarde" que Ostrin quiere evitar en tiempo de compilación.

`_` cubre explícitamente el resto de variantes cuando no interesa distinguirlas:

```ostrin
match shape {
    Circle(radius) => radius,
    _ => 0 m,
}
```

### 2.3 Guards (condiciones adicionales sobre el patrón)

```ostrin
match shape {
    Circle(radius) if radius > 10 m => "círculo grande",
    Circle(radius) => "círculo pequeño",
    _ => "otra forma",
}
```

Un guard (`if condición` después del patrón) reduce el conjunto de casos que ese patrón cubre; el compilador sigue exigiendo exhaustividad total contando el guard como no garantizado (por eso hace falta el segundo `Circle(radius) => ...` sin guard, para cubrir el resto de radios).

### 2.4 Patrones anidados y desestructuración

```ostrin
enum Tree<T> {
    Leaf
    Node(value: T, left: Tree<T>, right: Tree<T>)
}

fn depth<T>(tree: Tree<T>) -> Int {
    match tree {
        Leaf => 0,
        Node(value: _, left, right) => 1 + max(depth(left), depth(right)),
    }
}
```

- Los patrones se anidan libremente: se puede hacer `match` sobre un `Option<Result<T, E>>` desestructurando ambos niveles en una sola rama (`Some(Ok(v)) => ...`).
- `_` dentro de un patrón ignora ese campo sin darle nombre.

**Regla de desestructuración de campos nombrados** (aplica tanto a variantes de `enum` con campos nombrados como a `record`): un patrón `Variant(campo)` es azúcar de `Variant(campo: campo)` — une el campo llamado `campo` a un binding local con el **mismo nombre**. Para ligarlo a un nombre local distinto, se escribe explícito: `Variant(campo: otro_nombre)`. Por eso `Circle(radius)` (§1.4) funciona: el campo se llama `radius` y el binding local también se llama `radius`. Cuando una variante tiene varios campos, se puede mezclar la forma corta y la explícita libremente en el mismo patrón, como en `Node(value: _, left, right)` más abajo (`value: _` ignora ese campo explícitamente; `left`, `right` son la forma corta porque el nombre local coincide con el del campo).

Records también se desestructuran en patrones, con la misma regla:

```ostrin
match particle {
    Particle(mass: m, charge: c) if c == 0 C => "neutra, masa " + m.to_string(),
    Particle(mass: _, charge: _) => "cargada",
}
```

### 2.5 `match` sobre literales y rangos

```ostrin
fn classify(n: Int) -> String {
    match n {
        0 => "cero",
        1..10 => "un dígito positivo",
        n if n < 0 => "negativo",
        _ => "diez o más",
    }
}
```

- Patrones literales (`0`, `"texto"`, `true`) comparan por igualdad.
- Patrones de rango (`1..10`, inclusivo del límite inferior, exclusivo del superior — consistente con el resto del lenguaje una vez se cierre la sintaxis general de rangos) para tipos ordenables.

### 2.6 `if let` — azúcar para el caso de una sola variante

Cuando solo interesa un caso y el resto se ignora, exigir un `match` completo es ceremonia innecesaria:

```ostrin
if let Some(value) = maybe_value {
    print(value)
}
```

Equivale a `match maybe_value { Some(value) => { print(value) }, _ => {} }`, pero sin forzar a escribir la rama vacía. No reemplaza a `match`: sigue existiendo para cuando de verdad hacen falta varias ramas con valor de retorno.

---

## 3. Preguntas abiertas para la siguiente sesión de diseño

1. ~~Sintaxis general de rangos~~ — resuelto en el documento 06.
2. ~~`derive` para enums~~ — resuelto en el documento 12, §2.2.
3. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
