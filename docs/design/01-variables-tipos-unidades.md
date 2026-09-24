# Ostrin — Diseño core: Variables, Tipos y Unidades

Versión: 0.1 (borrador de diseño, previo a implementación)

Decisiones de fondo ya cerradas:
- Bindings **inmutables por defecto**.
- Tipado **estático con inferencia**.
- Sistema de unidades **estricto**: un escalar sin unidad no se combina con una cantidad física sin conversión explícita.

---

## 1. Variables

### 1.1 Declaración

```ostrin
temperature = 310 K
name = "hidrógeno"
count = 12
```

- `nombre = valor` crea un **binding inmutable**. El tipo se infiere del valor; no hace falta anotarlo.
- Reasignar un binding inmutable es error de compilación:

```ostrin
count = 12
count = 13
```
```text
Error OSTRIN-E1001
Cannot reassign immutable binding 'count'.
Declared as immutable at line 1.
Use 'mut count = ...' if reassignment is intended.
```

### 1.2 Mutabilidad explícita

```ostrin
mut count = 12
count = 13        // OK
```

- `mut` se declara una vez, en el punto de creación del binding. No existe "convertir" un binding inmutable en mutable después.
- Reasignar un `mut` debe conservar el tipo (y, si es una cantidad física, la dimensión — no la unidad exacta, ver §3.4).

### 1.3 Anotación de tipo explícita (opcional)

```ostrin
mass: Float = 70.5
velocity: Quantity<Velocity> = 3 m / 1 s
```

Se usa solo cuando se quiere fijar un tipo distinto del que la inferencia elegiría por defecto (p. ej. forzar `Float` en vez de `Int`), o en firmas de función.

### 1.4 Scope y shadowing

- Scope léxico, delimitado por bloques `{ }` (funciones, `if`, `for`, etc.).
- Se permite **shadowing** dentro del mismo scope: volver a escribir `nombre = ...` crea un binding nuevo que oculta el anterior. Es la forma idiomática de encadenar transformaciones sin necesitar `mut`:

```ostrin
data = read_raw()
data = normalize(data)
data = filter_outliers(data)
```

Esto no es mutación: son tres bindings distintos con el mismo nombre. El anterior sigue existiendo si algo lo capturó (closures), simplemente el nombre `data` ya no lo referencia hacia adelante.

---

## 2. Tipos

### 2.1 Tipos primitivos

| Tipo      | Descripción                          | Ejemplo        |
|-----------|---------------------------------------|----------------|
| `Int`     | Entero con signo (64 bits por defecto)| `12`           |
| `Float`   | Punto flotante (64 bits, IEEE-754)    | `12.5`         |
| `Bool`    | `true` / `false`                      | `true`         |
| `Char`    | Un carácter Unicode                   | `'h'`          |
| `String`  | Cadena UTF-8                          | `"hola"`       |
| `Void`    | Ausencia de valor (retorno de función)| —              |

No existe `null`/`nil`. La ausencia de valor se modela con `Option<T>` (§2.3).

### 2.2 Cantidades físicas: `Quantity<D>`

Un literal numérico seguido de un símbolo de unidad no produce un `Float` o `Int`: produce un `Quantity<D>`, donde `D` es una **dimensión** (no una unidad concreta).

```ostrin
temperature = 310 K       // tipo: Quantity<Temperature>
distance    = 5 nm        // tipo: Quantity<Length>
```

- `Quantity<D>` es un tipo **distinto** de `Float`/`Int`. Un número puro (`12`) nunca es implícitamente un `Quantity`.
- Internamente, `Quantity<D>` guarda: un valor numérico, la unidad en la que fue expresado, y `D` como vector de exponentes sobre las dimensiones base (ver §3). El compilador resuelve `D` en tiempo de compilación; el valor y la unidad concreta sí viven en runtime (para poder imprimir `"5 nm"` en vez de solo el número).
- El valor numérico interno de un `Quantity<D>` es **siempre `Float`**, sin importar si el literal se escribió con forma de entero (`5 nm` guarda `5.0`, no `5`). Las cantidades físicas son, salvo casos de conteo puro (que se modelan con `Int` sin unidad), magnitudes continuas — fijar `Float` evita sorpresas de división entera al operar (p. ej. que `10 s / 3` trunque en vez de dar un resultado fraccionario).

### 2.2.1 `Unit<D>`: una unidad como valor

Además de aparecer pegado a un número (`5 nm`), un símbolo de unidad reconocido es también, por sí solo, una **expresión de tipo `Unit<D>`** — el valor que representa "la unidad `nm`" (no una cantidad, solo la unidad):

```ostrin
km              // expresión de tipo Unit<Length>
```

Esto es lo que permite pasar una unidad como argumento normal a una función, como en `convert(5 nm, target: km)` (documento 02, §3.2): `km` ahí no es una cantidad, es un valor `Unit<Length>` que la función usa para saber a qué unidad convertir.

### 2.3 Tipos compuestos

```ostrin
List<Int>
Tuple(Float, String)
Option<Quantity<Length>>
Result<Float, String>

record Particle {
    mass: Quantity<Mass>
    charge: Quantity<Charge>
}
```

- `Option<T>`: reemplaza `null`. Valores: `Some(x)` / `None`.
- `Result<T, E>`: reemplaza excepciones para errores esperables (E/S, parseo, validación). El manejo de errores no recuperables (bugs, invariantes rotas) queda fuera de este documento.
- `record`: tipo producto con campos nombrados, estructuralmente tipado por nombre (nominal, no estructural — dos records con los mismos campos pero distinto nombre son tipos distintos).

---

## 3. Sistema de unidades

### 3.1 Dimensiones base

Ostrin fija 7 dimensiones base (SI) más 2 pragmáticas para uso general fuera de ciencia pura:

```text
Length              (m)
Mass                (kg)
Time                (s)
Temperature         (K)
ElectricCurrent     (A)
AmountOfSubstance   (mol)
LuminousIntensity   (cd)

Currency            (definida por el usuario/config regional, no fija tasas)
Information         (bit)
```

Toda unidad reconocida por el compilador se resuelve a un **vector de exponentes** sobre estas 9 dimensiones base. Por ejemplo:

```text
m/s        → Length^1 · Time^-1
m/s^2      → Length^1 · Time^-2
N (newton) → Mass^1 · Length^1 · Time^-2
mol/L      → AmountOfSubstance^1 · Length^-3
```

Dos cantidades son de la **misma dimensión** si y solo si sus vectores de exponentes son idénticos — el nombre que uses para la unidad no importa (`m/s` y `km/h` son la misma dimensión, `Velocity`).

### 3.2 Literales con unidad

```ostrin
310 K
5 nm
9.8 m/s^2
0.2 mmol/L
```

Gramática de la parte de unidad:

```text
unit_expr := unit_atom (('*' | '/') unit_atom)*
unit_atom := IDENT ('^' INT)?
```

- Requiere un espacio entre el número y la unidad (evita ambigüedad léxica con sufijos de tipo numérico).
- El identificador de unidad se resuelve contra una tabla de símbolos conocidos (stdlib) más las unidades definidas por el usuario (§3.5). Un símbolo no reconocido es error de compilación, no un identificador de variable:

```text
Error OSTRIN-E1010
Unknown unit 'nm3'.
Did you mean 'nm'?
```

### 3.3 Reglas aritméticas

| Operación                     | Regla                                                                 |
|--------------------------------|------------------------------------------------------------------------|
| `Quantity<A> + Quantity<A>`     | OK. Convierte al mismo múltiplo antes de sumar (ver §3.4).             |
| `Quantity<A> + Quantity<B>`, A≠B| Error de compilación (dimensiones incompatibles).                     |
| `Quantity<A> + Int/Float`       | Error de compilación (falta unidad explícita, ver abajo).             |
| `Quantity<A> * Quantity<B>`     | OK siempre. Resultado: `Quantity<A·B>` (exponentes se suman).          |
| `Quantity<A> / Quantity<B>`     | OK siempre. Resultado: `Quantity<A/B>` (exponentes se restan). Si A=B, resultado es `Float` puro (se cancela la dimensión). |
| `Quantity<A> * Int/Float`       | OK. Escala el valor, conserva la dimensión `A`.                        |
| `Quantity<A> / Int/Float`       | OK. Escala el valor (divide), conserva la dimensión `A` — dividir entre un escalar puro es solo un cambio de magnitud, no cambia de qué es cantidad. |
| `Quantity<A> == Quantity<A>`    | OK, compara tras normalizar unidades.                                  |
| `Quantity<A> == Quantity<B>`, A≠B| Error de compilación.                                                  |

Ejemplo del caso guía original:

```ostrin
a = 5 nm
b = 10 s
c = a + b
```
```text
Error OSTRIN-E1024
Invalid dimensional operation.
Cannot add:
    Length   (from 'a', 5 nm)
    Time     (from 'b', 10 s)
Expression: a + b
```

```ostrin
velocity = a / b     // Quantity<Length / Time>, se imprime como "0.5 nm/s"
```

Combinar escalar puro con cantidad física exige conversión explícita:

```ostrin
a = 5 nm
b = 3
a + b                     // Error OSTRIN-E1025: falta unidad para 'b'
a + (3 as nm)              // OK -> 8 nm
```

`as <unidad>` es la única forma de "inyectar" una unidad en un número puro; es una conversión explícita, nunca implícita. `<unidad>` no tiene que ser un símbolo literal escrito a mano (`nm`, `kg`) — es cualquier expresión de tipo `Unit<D>` (§2.2.1), incluida una variable o un parámetro de función:

```ostrin
fn inject<D: Dimension>(raw: Float, target_unit: Unit<D>) -> Quantity<D> {
    raw as target_unit
}

inject(5.0, nm)    // -> 5 nm, D = Length
inject(3.0, s)     // -> 3 s, D = Time
```

Esto es lo único que hacía falta para el caso genérico que en el documento 02 (§3.3, versión original) se intentaba resolver con una construcción `as D` aparte — no hace falta un mecanismo nuevo: `as` ya aceptaba cualquier `Unit<D>`, incluida una recibida como parámetro con `D` genérico. Ver documento 16 para el detalle completo de esta corrección.

### 3.4 Conversión entre unidades compatibles

Dentro de la misma dimensión, distintas unidades (m vs nm vs km) se convierten automáticamente usando factores conocidos por la stdlib:

```ostrin
a = 5 nm
b = 2 m
c = a + b      // OK, misma dimensión (Length) -> 2.000000005 m (o unidad que se defina como canónica de salida)
```

La unidad de salida por defecto es la del operando izquierdo; se puede forzar con `as`:

```ostrin
c = (a + b) as nm
```

### 3.5 Unidades definidas por el usuario

Para dominios fuera de física pura (finanzas, química con nombres locales, etc.):

```ostrin
unit USD : Currency
unit EUR : Currency
define 1 EUR = 1.08 USD

unit mmHg : Mass / (Length * Time^2)   // ejemplo de unidad derivada nombrada
```

- `unit X : D` declara `X` como unidad base de la dimensión `D` (si `D` ya existe) o crea una dimensión nueva (si el usuario también define `D` con `dimension D`).
- `define` registra un factor de conversión entre dos unidades de la misma dimensión.
- Las tasas de cambio de `Currency` **no** son fijas por el lenguaje — el usuario o una librería las provee y las actualiza; el sistema de tipos solo garantiza que no mezclas `USD` con `Length` por accidente, no que la tasa esté actualizada.

---

## 4. Preguntas abiertas para la siguiente sesión de diseño

1. **Funciones y firmas**: cómo se anota una función que es genérica sobre dimensión (p. ej. una función `to_kelvin` que acepta cualquier `Quantity<Temperature>`).
2. **Impresión/formato**: qué unidad "canónica" se usa al hacer `print()` de un `Quantity` resultante de una operación (¿la del operando izquierdo? ¿la unidad SI base?).
3. **Errores y control de flujo** (`Result`, propagación tipo `?`, panics vs errores recuperables) — no cubierto aquí.
4. **Lógica de comparación difusa** (`approximately ... tolerance ...`, `within ... .. ...`) — pertenece al sistema de expresiones/lógica, se diseña después de cerrar funciones.
5. **Concurrencia y modelo de memoria** — fuera de alcance de este documento.

---

## 3.6 Álgebra de unidades implementada (2026-09-24)

Estado real del compilador, que concreta §3.1–§3.5:

- **Catálogo** (`compiler/src/types.rs::unit_info`, replicado en `qty_runtime.c`): `m nm um mm cm km`,
  `s ns us ms min h day`, `kg g mg ug`, `K`, `A mA`, `mol mmol umol`, `cd`, `USD EUR`, `bit byte`,
  `L mL`, `Hz kHz`, `N kN`, `J kJ cal kcal`, `W kW`, `Pa kPa bar atm mmHg`, `C`, `V mV`, `ohm`.
  Cada símbolo tiene su dimensión en unidades base y su factor hacia la unidad coherente del SI
  (`atm` = 101 325 Pa; antes valía 1 Pa por error).
- **Dimensiones con nombre** en tipos: `Area`, `Volume`, `Velocity`/`Speed`, `Acceleration`,
  `Frequency`, `Force`, `Momentum`, `Energy`, `Power`, `Pressure`, `Density`, `Charge`, `Voltage`,
  `Resistance`, `Concentration`. Se expanden a dimensiones base: `Quantity<Energy>` y
  `Quantity<Mass * Length^2 / Time^2>` son el mismo tipo.
- **Forma canónica**: `*` y `/` entre cantidades agrupan exponentes (`kg*m/s*m/s` → `kg*m^2/s^2`) y
  funden átomos simples de la misma dimensión en el primero que aparece, ajustando el valor
  (`90 km/h * 30 min` → `45 km`). Si todo se cancela, el resultado es un número.
- **`as` con unidades compuestas** (`v as km/h`) y comprobación de dimensión: `x as s` con `x` una
  longitud es E1026. Los mensajes muestran la dimensión legible: `Mass*Length^2/Time^2 (Energy)`.
- **Introspección**: `q.value()` (número en su propia unidad) y `q.unit()` (texto de la unidad),
  usados por `std.viz` para etiquetar ejes.

Pendiente: `unit`/`dimension`/`define` declarados por el usuario, arrays de cantidades y unidades
afines (°C).

