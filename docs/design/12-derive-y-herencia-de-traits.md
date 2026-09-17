# Ostrin — Diseño core: `derive` y Herencia entre Traits

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [03-traits.md](03-traits.md)

Ambos temas cierran pendientes explícitos del documento 03 (§7, puntos 1 y 2). No hay decisiones de fondo nuevas que requieran tu confirmación aquí: son extensiones directas del sistema de traits ya cerrado, reutilizando patrones de sintaxis que ya existían (`+` para combinar bounds, la regla de coherencia, el impl nominal) en vez de inventar mecanismos nuevos.

---

## 1. Herencia entre traits (supertraits)

```ostrin
trait Ord: Eq {
    fn compare(self, other: Self) -> Ordering

    fn less_than(self, other: Self) -> Bool {
        self.compare(other) == Less
    }
}
```

- `trait Ord: Eq` declara que **cualquier tipo que implemente `Ord` debe implementar `Eq` primero**. `Eq` es el "supertrait" de `Ord`.
- Dentro de `Ord` (en sus métodos default), se puede asumir y llamar cualquier método de `Eq` sobre `Self`, exactamente igual que si `Ord` los tuviera declarados — porque el compilador ya garantiza que existen.
- Varios supertraits se combinan con `+`, igual que los trait bounds de genéricos (documento 03, §3):

```ostrin
trait Numeric: Add + Eq + Ord {
    ...
}
```

### 1.1 Implementar un trait con supertraits

```ostrin
record Distance {
    meters: Float
}

impl Eq for Distance {
    fn equals(self, other: Self) -> Bool { self.meters == other.meters }
}

impl Ord for Distance {
    fn compare(self, other: Self) -> Ordering {
        if self.meters < other.meters { Less }
        else if self.meters > other.meters { Greater }
        else { Equal }
    }
}
```

Si se intenta `impl Ord for Distance` sin que exista ya un `impl Eq for Distance`, es error de compilación:

```text
Error OSTRIN-E1050
Cannot implement 'Ord' for 'Distance': missing required supertrait 'Eq'.
'Ord' requires 'Eq' to be implemented first.
```

El orden en el código fuente no importa (el `impl Eq` puede estar antes o después en el archivo, incluso en otro módulo) — lo que importa es que exista en algún punto visible antes de que el compilador termine de resolver el programa completo.

### 1.2 Colisión de nombres entre supertraits

Si dos supertraits combinados con `+` declaran un método con el mismo nombre, llamarlo sin más sobre `self` es ambiguo:

```ostrin
trait Foo { fn describe(self) -> String }
trait Bar { fn describe(self) -> String }

trait Both: Foo + Bar {
    fn show(self) -> String {
        self.describe()   // Error: ambiguo, ¿Foo.describe o Bar.describe?
    }
}
```

Se desambigua calificando con el nombre del trait:

```ostrin
fn show(self) -> String {
    Foo::describe(self)
}
```

`Trait::metodo(valor)` es la forma general de llamar un método de un trait específico saltándose la resolución automática — también sirve, fuera del caso de colisión, para forzar explícitamente "quiero la versión de este trait" cuando por algún motivo hubiera más de una forma de leer una llamada.

## 2. `derive`

Muchos traits simples (igualdad estructural, orden, impresión) se implementarían casi siempre de la misma forma mecánica campo por campo. Escribirlos a mano en cada `record`/`enum` es ceremonia sin valor. `derive` genera esos `impl` automáticamente:

```ostrin
record Particle: Eq, Printable {
    mass: Quantity<Mass>
    charge: Quantity<Charge>
}
```

- La lista después de `:` en la declaración de un `record`/`enum` (misma posición sintáctica que los supertraits de un `trait`, §1) pide al compilador que **genere** los `impl` correspondientes con un algoritmo fijo, en vez de que el programador los escriba a mano.
- **Esto no es una excepción a "impl nominal explícito"** (documento 03, decisión de fondo): `derive` literalmente escribe el `impl Eq for Particle { ... }` en el momento de compilar — el `impl` existe igual que si se hubiera tecleado, solo que su cuerpo lo genera el compilador siguiendo una regla conocida. Nominal sigue significando "hace falta un impl real", no "hace falta teclearlo a mano".
- Se puede combinar con supertraits normales en el mismo tipo, y con genéricos: `record Vector<T: Add>: Eq, Printable { x: T, y: T }`.

### 2.1 Traits derivables en esta versión

Un conjunto fijo definido por la stdlib, no extensible por el usuario todavía (ver §2.5):

| Trait | Qué genera |
|---|---|
| `Eq` | `equals(self, other)`: compara **todos** los campos con `==`, en orden de declaración; `true` solo si todos coinciden. |
| `Ord` | `compare(self, other)`: orden lexicográfico por campos en orden de declaración (para `enum`, primero por orden de declaración de la variante, luego por sus campos). |
| `Printable` | `to_display(self) -> String`: `"NombreDelTipo { campo1: valor1, campo2: valor2 }"`, usando el `to_display` de cada campo. |
| `Default` | Un valor "por defecto" del tipo — ver §2.3. |
| `Hash` | Añadido en el documento 13 junto con `Map`/`Set`: combina el hash de todos los campos, en orden de declaración. |

Requisito: cada campo del tipo debe, a su vez, implementar el trait que se deriva (para `derive(Eq)`, cada campo debe implementar `Eq`). Si no, error de compilación señalando exactamente qué campo falla:

```text
Error OSTRIN-E1051
Cannot derive 'Eq' for 'Particle': field 'charge' has type 'Charge',
which does not implement 'Eq'.
Add 'impl Eq for Charge' or remove 'charge' from the derive.
```

### 2.2 `derive(Ord)` en un `enum`

```ostrin
enum Ordering: Eq, Ord {
    Less
    Equal
    Greater
}
```

Orden dado por la posición de declaración: `Less < Equal < Greater`. Para una variante con datos, primero se compara la posición de la variante, y solo si coincide se comparan sus campos (igual que el `derive(Ord)` estándar de la mayoría de lenguajes con este mecanismo).

### 2.3 `derive(Default)` y valores por defecto de campo

```ostrin
record Config: Default {
    retries: Int = 3
    timeout: Quantity<Time> = 30 s
    verbose: Bool = false
}

c = Config.default()          // Config { retries: 3, timeout: 30 s, verbose: false }
```

- Un campo puede declarar un **valor por defecto** (`campo: Tipo = valor`), independientemente de si el tipo deriva `Default` — es útil también para construir el valor omitiendo ese campo:

```ostrin
c = Config { verbose: true }   // retries y timeout toman su valor por defecto; verbose se sobrescribe
```

- `derive(Default)` genera una función asociada `Type.default() -> Type` que usa el valor por defecto declarado de cada campo, o —si un campo no tiene uno explícito— el `Default` del tipo de ese campo (si lo tiene; si no, error de compilación pidiendo un valor por defecto explícito para ese campo).

### 2.4 `derive` sí es coherente con la regla de un solo `impl` por trait

Como `derive(Eq)` genera exactamente un `impl Eq for Type`, escribir además un `impl Eq for Type` a mano en el mismo tipo es la misma colisión ya prohibida por la regla de coherencia (documento 03, §2):

```text
Error OSTRIN-E1052
'Particle' already has 'Eq' via 'derive'. Remove the derive or the manual 'impl', not both.
```

### 2.5 Qué queda fuera de esta versión

`derive` **no** es extensible por el usuario en este diseño — no existe todavía una forma de que un trait definido por el propio programador (o una librería) participe en `derive`. Eso requeriría algún tipo de sistema de macros/generación de código en tiempo de compilación, que es un tema propio (mencionado en la idea original como "Ostrin Compiler"/futuro sistema de anotaciones) y queda fuera de alcance de este documento.

---

## 3. Preguntas abiertas para la siguiente sesión de diseño

1. ~~`Hash`~~ — resuelto en el documento 13, junto con `Map`/`Set`.
2. **Extensibilidad de `derive`** para traits de usuario — depende de un futuro sistema de macros, fuera de alcance por ahora.
3. ~~`dyn Trait`~~ — resuelto en el documento 15. Sigue abierto: `select` sobre canales.
