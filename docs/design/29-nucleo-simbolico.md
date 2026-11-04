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

Los racionales usan `Int` de 64 bits con comprobación de desbordamiento. Si el resultado exacto no
cabe, el almacén se marca como fallido (`symbolic.status`) y `diff`/`eval` devuelven ese error tipado;
nunca se redondea ni se envuelve en silencio. La comprobación es conservadora (rechaza productos cuyo
valor en coma flotante supera 9·10¹⁸). Validado con el número armónico H₂₀ = 55835135/15519504.

`BigInt`, `Rational` de precisión arbitraria y multiprecisión quedan para una entrega propia (H2); este
núcleo fija la semántica de promoción y errores que esa entrega deberá respetar.

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

## 7. Validación

- 28 comprobaciones de derivadas simbólicas contra diferencias centrales (algoritmo independiente).
- Expansión y sustitución contra evaluación directa.
- Identidades canónicas, H₂₀ exacto, detección de desbordamiento y las reglas condicionadas por
  supuestos (también los casos en que **no** deben dispararse).
- Workflow completo: modelo `x(t) = A·exp(−g·t)·cos(w·t)`, derivadas primera y segunda, residuo de la
  EDO `x'' + 2g·x' + (g² + w²)·x = 0` por debajo de 10⁻¹² en 240 muestras, y figura SVG real de
  `std.viz`.
- Paridad intérprete/nativo, cero fallbacks al AST (`ir=62`) y cero asignaciones vivas.

## 8. Límites actuales

No hay factorización, resolución de ecuaciones, integración simbólica, matrices simbólicas, unidades
dentro de las expresiones (H4), generación de código ni operadores sobrecargados (`+`, `*`). La
validación contra un motor externo (por ejemplo SymPy) no se ha ejecutado. El almacén plano se mantiene; un enum recursivo nativo (`enum Expr { Num(Int) Add(Expr, Expr) }`, ver
`examples/recursive_enum.ostrin`) ya compila y es una representación alternativa para código simbólico futuro.
