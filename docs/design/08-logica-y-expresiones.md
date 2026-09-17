# Ostrin — Diseño core: Sistema de Lógica y Expresiones

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [06-rangos-e-iteradores.md](06-rangos-e-iteradores.md)

Este documento sigue la misma regla que ya se aplicó en `try` (documento 04) y en `to`/`until` (documento 06): **símbolo cuando es notación matemática universal y sin ambigüedad; palabra cuando un símbolo introduciría ambigüedad, se pasaría por alto al leer, o no tiene una lectura obvia en voz alta.**

---

## 1. Comparación — símbolos (notación matemática estándar)

```ostrin
a == b
a != b
a < b
a > b
a <= b
a >= b
```

Sin cambios respecto a la notación matemática que cualquiera ya conoce — aquí un símbolo no genera ambigüedad ni se olvida fácilmente, así que no hay razón para reemplazarlo por palabras.

- Comparar dos `Quantity<D>` exige la misma dimensión `D` (documento 01, §3.3); comparar tipos no relacionados es error de compilación, no `false` silencioso.
- `==`/`!=` solo están disponibles para tipos que implementan `Eq` (documento 03); `<`, `>`, `<=`, `>=` requieren `Ord`.

### 1.1 Sin comparaciones encadenadas

Ostrin **no** soporta `0 < x < 10` como azúcar de `0 < x and x < 10` (a diferencia de Python). Para ese caso existe una forma más legible y ya definida en el documento 06:

```ostrin
x within (0 to 10)
```

Se prefiere `within` sobre encadenar comparadores porque deja explícito que se trata de una comprobación de rango (con su propia semántica de inclusión/exclusión vía `to`/`until`), en vez de una cadena de comparaciones genérica que además complicaría la gramática del parser sin necesidad real.

## 2. Lógicos — palabras, con cortocircuito

```ostrin
if temperature > 300 K and pressure < 2 atm {
    ...
}

if not is_valid(sample) {
    ...
}

ready = has_data or has_cache
```

- `and`, `or`, `not` en vez de `&&`, `||`, `!`. Mismo razonamiento que `try`/`to`/`until`: `!is_valid(x)` es fácil de leer como "is_valid(x)" al pasar rápido la vista por el código si el `!` queda pegado sin espacio; `not is_valid(x)` no se puede leer mal.
- `and`/`or` evalúan con **cortocircuito**: en `a and b`, si `a` es `false`, `b` no se evalúa; en `a or b`, si `a` es `true`, `b` no se evalúa. Relevante cuando `b` tiene efectos secundarios o es costoso de calcular.
- Solo operan sobre `Bool`. No existe "truthiness" implícito: un `Int`, `String` o `List` nunca se evalúa como condición sin una comparación explícita que produzca `Bool` (`count > 0`, no `count`; `not items.is_empty()`, no `items`). Consistente con la regla general de no-coerción-implícita ya establecida en el documento 01 (§3.3) para unidades y extendida aquí a lógica.

## 3. Condicionales como expresión

Ya introducido en documentos anteriores (`if` en el documento 02, `match` en el 05) — no hace falta un operador ternario (`cond ? a : b`) aparte, porque `if`/`else` ya funcionan como expresión:

```ostrin
label = if temperature > 373 K { "vapor" } else { "líquido" }
```

Un `if` usado como expresión (no como sentencia) exige la rama `else` — omitirla es error de compilación, porque sin `else` no habría valor definido para el caso en que la condición es falsa.

## 4. Pertenencia a rango — `within`

Ya definido formalmente en el documento 06, §1.1:

```ostrin
if temperature within (300 K to 320 K) {
    ...
}
```

`a within r` requiere que `r` sea `Range<D>` de la misma dimensión que `a`, y desazucara a `r.contains(a)`.

## 5. Igualdad con tolerancia — `approximately ... tolerance ...`

Comparar cantidades físicas con `==` exacto casi nunca es lo que se quiere en ciencia (error de medición, redondeo de punto flotante). Ostrin lo hace explícito como su propio operador, en vez de dejar que el programador reimplemente `abs(a - b) < epsilon` a mano cada vez —y se equivoque con el epsilon, o lo olvide:

```ostrin
if concentration approximately 5 mmol/L tolerance 0.2 mmol/L {
    ...
}
```

- `a approximately b tolerance t` ⟺ `abs(a - b) <= t`.
- `a`, `b` y `t` deben ser de la **misma dimensión** — comparar con una tolerancia de otra dimensión es tan inválido como sumar magnitudes distintas (mismo error de compilación conceptual que el documento 01, §3.3).
- **`tolerance` es obligatorio, sin valor por defecto.** `a approximately b` sin `tolerance` es error de compilación:

```text
Error OSTRIN-E1090
'approximately' requires an explicit 'tolerance'.
Ostrin does not assume a default epsilon — specify the acceptable margin explicitly.
```

Se decide así porque un epsilon implícito "razonable" no existe de forma universal (depende completamente de la escala y el dominio: un margen aceptable en nanómetros no lo es en años luz), y elegir uno por defecto escondería una decisión científica relevante dentro del lenguaje.

### 5.1 Tolerancia relativa (`%`)

Para cuando el margen aceptable es un porcentaje del valor esperado, no una magnitud absoluta:

```ostrin
if concentration approximately 5 mmol/L tolerance 5% {
    ...
}
```

- `N%` es un literal de tipo `Percent` (una razón adimensional, `N / 100`), utilizable como tolerancia relativa: `a approximately b tolerance N%` ⟺ `abs(a - b) <= b * (N / 100)`.
- `Percent` no se puede sumar ni comparar directamente con un `Quantity<D>` fuera de este contexto — es un tipo propio con un único uso definido aquí y en cualquier otro lugar de la stdlib que explícitamente lo acepte (evita que `%` se convierta en una forma encubierta de mezclar escalares con cantidades físicas, algo que el documento 01 ya prohíbe).

## 6. Precedencia de operadores

De mayor a menor precedencia (se evalúa primero lo de arriba):

```text
1.  llamada a función, acceso a campo (.), indexado ([])
2.  ^ (exponente de unidad, ej. m^2 — solo dentro de una expresión de unidad, documento 01 §3.2)
3.  unario: - (negación numérica), not
4.  * / (multiplicación, división — incluye composición de dimensiones)
5.  + - (suma, resta)
6.  as (conversión explícita de unidad, ej. a as nm — documento 01 §3.3)
7.  to, until, step (construcción de rangos — documento 06 §1)
8.  == != < > <= >=
9.  within, approximately ... tolerance ...
10. and
11. or
12. = (asignación — solo válida como sentencia, no anidable en una expresión)
```

- `as` liga más flojo que la aritmética y más fuerte que la comparación: `(a + b) as nm == c` se lee "convierte el resultado de `a + b` a nm, y compara ese resultado con `c`" sin necesitar los paréntesis alrededor de `a + b` — aunque, igual que con `and`/`or`, se recomiendan paréntesis explícitos cuando `as` participa en una expresión con varios operadores, precisamente por ser una de las precedencias menos intuitivas a primera vista.
- `to`/`until`/`step` ligan más fuerte que `within` y que las comparaciones: construyen primero el `Range<T>` completo, y ese rango es lo que participa después en `within` o en una comparación. Así, `x within 0 to 10 and y > 0` se parsea como `x within (0 to 10) and (y > 0)`, sin paréntesis extra alrededor del rango.

- `and` liga más fuerte que `or` (estándar: permite escribir `a and b or c and d` sin paréntesis y que signifique `(a and b) or (c and d)`), pero se recomienda paréntesis explícitos en cualquier combinación de tres o más términos lógicos para legibilidad, sin que el lenguaje lo exija.
- `within` y `approximately ... tolerance ...` se sitúan al mismo nivel que las comparaciones en la intención (son formas de comparación), pero se evalúan después de aritmética, antes de `and`/`or` — permite escribir `a within (0 to 10) and b > 5` sin paréntesis extra alrededor de cada mitad.

---

## 7. Preguntas abiertas para la siguiente sesión de diseño

1. **`Percent` como tipo general**: si merece existir fuera del contexto de `tolerance` (p. ej. como resultado de divisiones, `(part / total) as Percent`) — se dejó acotado a `tolerance` por ahora.
2. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
