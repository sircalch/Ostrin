# 33 · Diferenciación automática (`std.autodiff`)

Estado: **primer corte implementado** (`std.autodiff`, PR #274). Calcula gradientes de primer orden de funciones
escalares de `Float` por modo inverso (reverse mode), usando una cinta (tape) de nodos. El módulo no puede aplicarse a
una función arbitraria del usuario: solo deriva las cuatro expresiones fijas que define el propio módulo, y el API
genérico de la cinta sirve para construir otras expresiones a mano. El mismo código se ejecuta en el intérprete y en el
backend nativo, y ambos deben producir la misma salida que un oráculo independiente.

Este documento describe el código real: `compiler/std/autodiff.ostrin`, `examples/autodiff_reverse.ostrin`, el oráculo
`scripts/autodiff-oracle.mjs` y su fichero de referencia `compiler/tests/fixtures/autodiff_reverse_oracle.txt`.
Complementa al documento 32 y no sustituye a la diferenciación hacia adelante de `examples/autodiff_project`, que
no se modifica (§7).

## 1. Alcance y decisión

Ostrin no tiene clausuras con captura ni trazado de valores de función. Por eso no existe `grad(f, x)` sobre funciones
arbitrarias del usuario: una función con nombre de nivel superior es un valor de función, pero el módulo no puede
registrar sus operaciones en la cinta. La solución adoptada es doble:

- `reverse_grad_2` deriva un conjunto cerrado de cuatro expresiones de dos variables (`expr_name`, §5).
- El API genérico (`tape_new`, `tape_var`, `tape_op1`, `tape_op2`, `tape_backward`, `tape_grad`) permite construir
  otras expresiones nodo a nodo, pero lo hace quien llama, no el compilador.

El módulo solo calcula primeras derivadas. No hay derivadas de orden superior ni salidas vectoriales.

## 2. Representación: la cinta

Una cinta (`Tape`) es una lista de nodos en orden de evaluación. Cada nodo (`Node`) guarda:

- `op`: la etiqueta de la operación (`String`), por ejemplo `"mul"` o `"sin"`. Los nodos de entrada tienen la
  etiqueta `"var"`.
- `value`: el valor del nodo (`Float`).
- `parents`: los índices (`List<Int>`) de sus nodos padre en la cinta. Un nodo de entrada no tiene padres.
- `partials`: las derivadas locales (`List<Float>`), donde `partials[k]` es `d value / d parent k`.

La cinta guarda además `error` (`String`): es `""` mientras todos los valores y derivadas locales son finitos, y
guarda el primer problema encontrado en otro caso (§6).

Un solo recorrido hacia atrás basta para el gradiente completo. `tape_backward(t, out)` siembra el adjunto del nodo
`out` con `1.0` y recorre los nodos desde `out` hasta el índice `0`. En cada nodo reparte `adj[i] * partials[k]` a cada
padre. Los nodos posteriores a `out` no se visitan y quedan con adjunto `0.0`.

## 3. Funciones y registros públicos

Nombres y firmas verificados con `grep "^pub"` sobre `compiler/std/autodiff.ostrin`:

| Función | Firma | Semántica |
| --- | --- | --- |
| `tape_new` | `() -> Tape` | Cinta vacía con `error = ""`. |
| `tape_len` | `(t: Tape) -> Int` | Número de nodos. |
| `tape_last` | `(t: Tape) -> Int` | Índice del último nodo añadido (`-1` si la cinta está vacía). |
| `tape_var` | `(t: Tape, x: Float) -> Tape` | Añade una variable de entrada con valor `x`, sin padres. |
| `tape_op2` | `(t: Tape, op: String, i: Int, j: Int) -> Tape` | Añade un nodo binario sobre los nodos `i` y `j`. |
| `tape_op1` | `(t: Tape, op: String, i: Int, k: Float) -> Tape` | Añade un nodo unario sobre el nodo `i`. `k` es la constante de `pow`, `addk` y `mulk`; las demás operaciones la ignoran. |
| `tape_value` | `(t: Tape, i: Int) -> Float` | Valor del nodo `i`. No comprueba límites ni `t.error`. |
| `tape_backward` | `(t: Tape, out: Int) -> Result<List<Float>, String>` | Adjuntos `d out / d nodo` para todos los nodos. |
| `tape_grad` | `(t: Tape, out: Int, vars: List<Int>) -> Result<List<Float>, String>` | Gradiente de `out` respecto a los nodos listados en `vars`. |
| `expr_count` | `() -> Int` | Número de expresiones fijas: `4` (tipos 0 a 3). |
| `expr_name` | `(kind: Int) -> String` | Nombre de la expresión `kind`; `"unknown"` si no existe. |
| `expr_value` | `(kind: Int, x: Float, y: Float) -> Result<Float, String>` | Valor de la expresión evaluada directamente en `Float`, sin cinta. Es la referencia para diferencias finitas. |
| `reverse_grad_2` | `(kind: Int, x: Float, y: Float) -> Result<List<Float>, String>` | `(d f/d x, d f/d y)` de la expresión `kind` en `(x, y)`, por modo inverso. |
| `reverse_value_2` | `(kind: Int, x: Float, y: Float) -> Result<Float, String>` | Valor de la expresión leído de la cinta, para comprobar el pase hacia adelante. |

Registros públicos:

- `Node { op: String, value: Float, parents: List<Int>, partials: List<Float> }`
- `Tape { nodes: List<Node>, error: String }`

Son privadas `is_finite_value`, `push_node`, `cube` y `build_expr`. `is_finite_value(x)` es
`x == x and abs(x) <= 1.7976931348623157e308`, es decir, rechaza NaN e infinitos.

## 4. Operaciones implementadas

Todas las operaciones se toman de `tape_op2` y `tape_op1`. Cualquier otra etiqueta se rechaza (§6).

Binarias (`tape_op2(t, op, i, j)`, valor sobre los nodos `a = value[i]` y `b = value[j]`):

| `op` | `value` | `partials` (respecto a `i`, respecto a `j`) |
| --- | --- | --- |
| `"add"` | `a + b` | `1.0`, `1.0` |
| `"sub"` | `a - b` | `1.0`, `-1.0` |
| `"mul"` | `a * b` | `b`, `a` |
| `"div"` | `a / b` | `1.0 / b`, `-a / (b * b)` |

Unarias (`tape_op1(t, op, i, k)`, sobre `a = value[i]`):

| `op` | `value` | `partial` respecto a `i` |
| --- | --- | --- |
| `"neg"` | `0.0 - a` | `-1.0` |
| `"sin"` | `sin(a)` | `cos(a)` |
| `"cos"` | `cos(a)` | `0.0 - sin(a)` |
| `"exp"` | `exp(a)` | el propio valor |
| `"ln"` | `ln(a)` | `1.0 / a` |
| `"pow"` | `pow(a, k)` (exponente constante `k`) | `k * pow(a, k - 1.0)` |
| `"addk"` | `a + k` | `1.0` |
| `"mulk"` | `k * a` | `k` |

Ni `pow` ni `ln` tienen tratamiento especial de dominio: `ln(0)` produce `-inf`, y el nodo se marca con error (§6).

## 5. Expresiones fijas

`expr_name` define cuatro expresiones de `(x, y)`. El nodo 0 de la cinta es `x` y el nodo 1 es `y`. Los nodos
siguientes son los que añade `build_expr`, en este orden:

- **Tipo 0**, `x*y + sin(x)`: nodo 2 `mul(0, 1)`, nodo 3 `sin(0)`, nodo 4 `add(2, 3)`.
- **Tipo 1**, `exp(x)*ln(y) - 2*(y/x)`: nodo 2 `exp(0)`, nodo 3 `ln(1)`, nodo 4 `mul(2, 3)`, nodo 5 `div(1, 0)`
  (es `y/x`), nodo 6 `mulk(5, 2.0)`, nodo 7 `sub(4, 6)`.
- **Tipo 2**, `(x-y)^3 + cos(y)*x`: nodo 2 `sub(0, 1)`, nodo 3 `pow(2, 3.0)`, nodo 4 `cos(1)`, nodo 5 `mul(4, 0)`,
  nodo 6 `add(3, 5)`.
- **Tipo 3**, `-(x*x + 1)/(y*y + 2)`: nodo 2 `mul(0, 0)`, nodo 3 `addk(2, 1.0)`, nodo 4 `mul(1, 1)`,
  nodo 5 `addk(4, 2.0)`, nodo 6 `div(3, 5)`, nodo 7 `neg(6)`.

`expr_value` calcula los mismos cuatro valores directamente en `Float`, sin cinta. Sirve como referencia para la
diferencia finita y para `reverse_value_2`, que compara el valor del pase hacia adelante con la misma expresión.

`reverse_grad_2` valida primero las entradas: si `x` o `y` no son finitos, devuelve `Err` antes de construir la cinta.
Después comprueba el tipo, construye la cinta con `tape_var` y `build_expr`, y llama a `tape_grad` con
`vars = [0, 1]`.

## 6. Errores y límites

Los fallos nunca abortan. El primer problema queda registrado en `t.error` y cada operación posterior lo conserva.

Mensajes de `Err` que produce el código, tal como están en el fuente:

- `reverse_grad_2 needs finite inputs`: `x` o `y` NaN o infinito. Lo devuelve `reverse_grad_2`, y también
  `reverse_value_2` con su propio texto, `reverse_value_2 needs finite inputs`.
- `reverse_grad_2: unknown expression kind`: `kind` fuera de `0..3`. Lo devuelven `reverse_grad_2` y
  `reverse_value_2`. `expr_value` devuelve el mismo texto para un tipo desconocido, aunque el nombre no sea el suyo.
- `autodiff: non-finite value or partial at node N`: el valor o alguna derivada local del nodo `N` no es finito.
  Es la primera falla de la cinta, por ejemplo `ln(0)`.
- `autodiff: operand index out of range`: un índice de operando fuera de rango, o una etiqueta de operación
  desconocida. Ver la nota de abajo.
- `tape_backward: output index out of range`: el índice `out` no existe en la cinta.
- `tape_grad: variable index out of range`: un índice de `vars` no existe en el adjunto.

Propiedades del control de errores:

- Si un índice de operando está fuera de rango, `tape_op1` o `tape_op2` añaden de todos modos un nodo con valor
  `0.0` y sin padres. Así los índices de los nodos posteriores no se desplazan.
- `tape_backward` devuelve `Err(t.error)` antes de hacer nada si la cinta ya tiene error.
- `tape_value` no comprueba nada. Quien la llame debe mirar `t.error` o pasar por `tape_backward`.

**Nota sobre el mensaje de etiqueta desconocida.** Cuando la etiqueta `op` no existe, el código toma la misma ruta que un
índice fuera de rango y produce `autodiff: operand index out of range`. El mensaje es engañoso en ese caso: el
problema es la etiqueta. El texto no se corrige en este documento, porque el fuente no se cambia aquí.

**Límite de la función sin capturas.** Como Ostrin no tiene clausuras con captura, `reverse_grad_2` no puede recibir una
función del usuario. Su conjunto de expresiones está fijado en `build_expr`.

## 7. Relación con la diferenciación hacia adelante

`examples/autodiff_project/autodiff/autodiff.ostrin` es un paquete independiente que implementa diferenciación hacia
adelante con números duales: `record Dual { v: Float, d: Float }`, con `derivative`, `value`, `gradient2` y `newton`.
Ese paquete no usa `std.autodiff` y no se modifica en este cambio.

Las dos vías calculan derivadas por reglas distintas: el paquete propaga pares `(valor, derivada)` por cada variable, y
`std.autodiff` registra la cinta y la recorre hacia atrás una vez. `gradient2` del paquete hace una pasada por variable
de entrada (dos pasadas para dos variables), mientras que `tape_backward` da el gradiente completo en un solo recorrido.
Este documento no mide esa diferencia de rendimiento.

## 8. Validación

Hay tres comprobaciones, con alcance distinto:

1. **Comprobación interna del ejemplo.** `examples/autodiff_reverse.ostrin` evalúa 12 puntos de las cuatro expresiones.
   En cada punto compara `reverse_grad_2` con una diferencia central del valor plano de `expr_value`, con paso
   `h = 1e-6` y tolerancia `|ad - fd| <= 1e-6 * max(1, |fd|)`. Además compara el valor de `reverse_value_2` con
   `expr_value`. Imprime `ok` o `MISMATCH`. Añade cuatro casos de error: entrada NaN, entrada infinita,
   `ln` de cero y tipo desconocido.
2. **Oráculo independiente.** `scripts/autodiff-oracle.mjs` usa derivadas cerradas escritas a mano, sin compartir código
   con el módulo, y la misma diferencia central. No usa paquetes externos. Produce el fichero de referencia
   `compiler/tests/fixtures/autodiff_reverse_oracle.txt`, de 20 líneas: 3 de cabecera, 12 de puntos (todas `ok`),
   1 de cabecera de errores y 4 de error.
3. **Prueba de integración.** `std_autodiff_reverse_matches_oracle_in_both_backends`, en `compiler/tests/examples.rs`,
   exige que la salida del intérprete y la del binario nativo coincidan con el fichero de referencia, y que el informe
   de tipos nativo muestre `ast-fallback: 0`.

Resultados de esta revisión:

- Ejecuté `node scripts/autodiff-oracle.mjs`. Su salida coincide con el fichero de referencia una vez normalizados los
  finales de línea. El fichero usa CRLF y el script escribe LF. La prueba de Rust hace la misma normalización antes de
  comparar (`replace("\r\n", "\n")`).
- El diagnóstico del oráculo, enviado a stderr, da `max relative error |closed form - central difference| / max(1,|fd|): 2.244e-10`.
  Es el mismo valor que consta en el PR #274. Compara la derivada cerrada con la diferencia central, no el gradiente
  de la cinta con la diferencia central.
- No ejecuté el intérprete ni el backend nativo de Ostrin en esta revisión. Que ambos produzcan el fichero de referencia
  lo afirma la prueba de integración del punto 3, y no lo he comprobado de nuevo aquí.

## 9. Límites actuales

- Solo funciones escalares de `Float`. `reverse_grad_2` tiene exactamente dos entradas, `x` e `y`.
- Solo primeras derivadas. El encabezado del módulo lo declara así.
- Sin trazado de funciones del usuario: el conjunto de expresiones es fijo (§1, §5).
- `tape_value` no comprueba límites ni errores (§6).
- La etiqueta de operación desconocida se reporta con un mensaje de índice fuera de rango (§6).
- `expr_name` devuelve `"unknown"` para un tipo inexistente, mientras que `expr_value` devuelve `Err` con el texto
  `reverse_grad_2: unknown expression kind`, que nombra a otra función.

## 10. Lo que no está hecho

- Modo inverso sobre funciones arbitrarias del usuario. Requiere clausuras con captura o trazado de valores de función.
- Derivadas de orden superior: no hay segundas derivadas ni hessianos.
- Salidas vectoriales: no hay jacobianos ni funciones de salida múltiple. `tape_grad` calcula el gradiente de un solo
  nodo de salida.
- Operaciones fuera de las listadas en §4: no hay `tan`, `sqrt`, `pow` con exponente variable, ni `abs` o `max`.
- Comprobación de límites en `tape_value`, y mensaje propio para etiquetas de operación desconocidas.
- Benchmarks de rendimiento frente al modo hacia adelante. No se ha medido nada.
- Integración con `examples/autodiff_project`. Ese paquete sigue usando números duales y no se toca.
