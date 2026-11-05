# 29 · Núcleo simbólico (`std.symbolic`)

Estado: **primer corte implementado**. Álgebra simbólica con aritmética racional exacta, supuestos
explícitos, diferenciación, sustitución, expansión y evaluación numérica, escrita en Ostrin y
ejecutada por el intérprete, el backend nativo y WASI con la misma salida.

## 1. Arquitectura

Las expresiones viven en un `Store`: una tabla de nodos direccionados por índice (un DAG), no un árbol
de enums recursivos. La elección tiene una razón medida: un `enum Expr { Add(Expr, Expr), ... }`
funciona en el intérprete pero **no compila en el backend nativo** (el C emitido contiene un campo de
tipo incompleto, porque los enums son uniones por valor). El almacén plano evita ese límite, da
*hash-consing* (subexpresiones iguales se comparten) y hace que la igualdad estructural sea una
comparación de enteros.

| Elemento | Representación |
| --- | --- |
| `Store` | listas paralelas (`kinds`, `nums`, `dens`, `names`, `starts`, `counts`, `pool`) y un `Map` de internado |
| `Expr` | `{ store, id }`, un manejador a un nodo |
| Nodos | número racional, símbolo, suma n-aria, producto n-ario, potencia, llamada a función |
| Funciones | `sin`, `cos`, `exp`, `ln` |

## 2. Canonicalización

Los constructores canonizan al construir: aplanan sumas y productos, ordenan operandos, pliegan
constantes de forma exacta, recogen términos semejantes (`2*x + x/2 = 5/2*x`) y factores repetidos
(`x*x = x^2`). No hay un paso `simplify` separado; la igualdad `same(a, b)` es válida porque las
formas son canónicas dentro de un almacén. El orden de impresión depende del orden de creación de los
nodos.

## 3. Números exactos

Los racionales se guardan como dos `Int` de 64 bits mientras caben y pasan a precisión arbitraria
(`std.bigint`, documento 30) en cuanto un resultado exacto no cabe, de modo que **no hay desbordamiento**:
`2^62 · 4`, `2^200` y la suma armónica H₆₀ = 15117092380124150817026911/3230237388259077233637600 son exactos.
Solo un denominador cero marca el almacén como fallido (`symbolic.status`), y `diff`/`eval` devuelven
entonces ese error tipado. Validado con H₂₀ (camino de 64 bits) y con H₆₀, 2²⁰⁰, 2⁶²·4 y un coeficiente
de 30 dígitos contra el `BigInt` de JavaScript. `big_number` y `rational_from_text` crean números de
cualquier longitud desde texto decimal. Los exponentes numéricos se evalúan solo hasta |e| ≤ 100000; más
allá la potencia se deja sin evaluar.

## 4. Supuestos y simplificaciones válidas

Todo símbolo es real. Se pueden declarar `positive`, `nonzero` e `integer`. Una regla solo se
aplica si es válida en el dominio declarado:

| Expresión | Sin supuestos | Con supuesto |
| --- | --- | --- |
| `x / x` | `x^0` (no se asume `x ≠ 0`) | `1` si `x` es positivo o no nulo |
| `exp(ln(x))` | sin cambio | `x` si `x` es positivo |
| `ln(exp(x))` | `x` (válido para todo real) | — |
| `(x^2)^(1/2)` | sin cambio (no es `x` para `x < 0`) | `x` si `x` es positivo |
| `0 * x` | `0` | convención: los símbolos son finitos |

La derivada de `x^x` exige una base positiva y devuelve un error tipado si no se declara.

## 5. Derivación

Reglas: suma, producto, potencia con exponente libre de la variable, exponente variable sobre base
positiva, y regla de la cadena para `sin`, `cos`, `exp` y `ln`. Una función sin regla (`tan`)
devuelve un error tipado en lugar de un resultado incorrecto.

## 6. Evaluación numérica y compilación

`eval(expr, nombres, valores)` evalúa recursivamente en `Float` y devuelve errores tipados (símbolo sin
valor, `ln` de un no positivo, base negativa con exponente no entero, división por cero). Es una
interpretación del DAG, no código generado: **la compilación de expresiones a funciones nativas no
está implementada**.

## 6a. Operadores

`+`, `-`, `*` y `/` construyen expresiones directamente, `-x` las niega y un `Int` a la izquierda se lee como un número
exacto (`2 * x`, `1 - x`); el operando derecho debe ser una `Expr` (`x + symbolic.number(s, 1)`), y las potencias siguen
siendo explícitas (`power`). Con formas canónicas, las identidades del cálculo se comprueban por igualdad
estructural: la regla del producto, de la suma y de la cadena sobre expresiones concretas.

## 6b. Dimensiones físicas

Un símbolo puede declararse con una dimensión (`unit_symbol(s, "v", "Length/Time")`). Las dimensiones son
vectores de exponentes racionales sobre dimensiones base con nombre (cualquier nombre sin dígito inicial ni caracteres
de sintaxis, de modo que `Base2` y `Ángulo` son válidos); `dimension_of(expr)` las propaga: una
suma exige dimensiones iguales, un producto suma exponentes, una potencia con exponente numérico los
multiplica (la raíz cuadrada los divide por dos) y `sin`, `cos`, `exp` y `ln` exigen un argumento
adimensional. El texto canónico ordena las dimensiones base alfabéticamente (`Length^2*Mass*Time^-2`) y
`"1"` es adimensional. Los constructores no fallan ante una incoherencia; la comprobación es la llamada
explícita a `dimension_of`. Las derivadas conservan la coherencia por construcción: `d(E)/dv` de una
energía cinética tiene dimensión `Length*Mass*Time^-1`, y la simplificación no elimina dimensiones sin
justificación (`E/E` solo da `1` si los factores son no nulos). Los valores se interpretan en unidades
coherentes. Aún no se conectan estas dimensiones con `Quantity<D>` del sistema de tipos estático.

## 7. Validación

- 28 comprobaciones de derivadas simbólicas contra diferencias centrales (algoritmo independiente).
- Expansión y sustitución contra evaluación directa.
- Identidades canónicas, H₂₀ y H₆₀ exactos, aritmética exacta más allá de 64 bits y las reglas condicionadas por
  supuestos (también los casos en que **no** deben dispararse).
- Workflow completo: modelo `x(t) = A·exp(−g·t)·cos(w·t)`, derivadas primera y segunda, residuo de la
  EDO `x'' + 2g·x' + (g² + w²)·x = 0` por debajo de 10⁻¹² en 240 muestras, y figura SVG real de
  `std.viz`.
- Paridad intérprete/nativo, cero fallbacks al AST (`ir=67`) y cero asignaciones vivas.

## 8. Límites actuales

No hay factorización, resolución de ecuaciones, integración simbólica, matrices simbólicas ni generación de código. La
validación contra un motor externo (por ejemplo SymPy) no se ha ejecutado. El almacén plano se mantiene; un enum recursivo nativo (`enum Expr { Num(Int) Add(Expr, Expr) }`, ver
`examples/recursive_enum.ostrin`) ya compila y es una representación alternativa para código simbólico futuro.
