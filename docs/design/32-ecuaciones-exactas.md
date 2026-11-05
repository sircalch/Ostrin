# 32 · Ecuaciones exactas (`std.solve`)

Estado: **primer corte implementado** (`std.solve`, PR #269). Resuelve exactamente ecuaciones lineales `a·x + b = 0`,
cuadráticas `a·x² + b·x + c = 0` y las raíces racionales de un polinomio de coeficientes enteros. Las raíces se
devuelven como texto, de modo que ninguna raíz pasa por coma flotante. El mismo código se ejecuta en el intérprete y
en el backend nativo, y ambos deben producir el mismo fichero de salida que un oráculo independiente.

Este documento describe el código real: `compiler/std/solve.ostrin` y `examples/solve_core.ostrin`. Complementa al
documento 31, que trata `std.polynomial`, del que `std.solve` depende para las raíces racionales.

## 1. Alcance y decisión

El módulo resuelve ecuaciones de una variable, con coeficientes enteros y raíces exactas. No hay raíces complejas,
no hay sistemas de ecuaciones y no hay fórmulas cerradas para grado tres o superior. Lo que el módulo no puede
expresar exactamente devuelve `Err` o un texto documentado (§6); nunca aborta ni devuelve una aproximación en silencio.

La aritmética de la rama cuadrática usa solo `Int`. Por eso los coeficientes se limitan a `|coeff| <= 10^6` (§4).
Con esa cota el discriminante `b·b - 4·a·c` queda por debajo de `5·10^12` y no desborda.

## 2. Representación de los resultados

Todas las raíces son cadenas. Hay tres formas posibles:

- **Racional `n/d`**: en términos irreducibles, con `d > 0`. El cero es `0/1`; los enteros llevan denominador `/1`.
  Ejemplo: `solve_linear(3, -6)` produce `2/1`, y `solve_linear(6, -4)` produce `2/3`.
- **Cuadrática irracional `(p + sqrt(q))/d`**, con `q` libre de cuadrados y, cuando el coeficiente del radical no es
  uno, `(p + k*sqrt(q))/d`. Se usa `" - "` en lugar de `" + "` cuando el término del radical es negativo.
- **Regla de normalización** (la misma que fija el oráculo en su cabecera, `scripts/solve-oracle.mjs`):
  1. Se escribe `D = k²·q` con `q` libre de cuadrados y `q > 1`, donde `k` es el mayor entero con `k²` dividiendo a `D`.
  2. Se toma `P = -b` y `d = 2a`. Si `d < 0`, se cambian los signos de `P` y de `d`. Así el conjunto de las dos raíces
     no cambia.
  3. Se calcula `g = gcd(|P|, k, d)` y se dividen `P`, `k` y `d` por `g`. Queda `d > 0` y `gcd(|P|, k, d) = 1`.
  4. Las dos raíces son `(P - S)/d` y `(P + S)/d`, en ese orden, donde `S = sqrt(q)` si `k = 1` y `S = k*sqrt(q)` si `k > 1`.
     `P` y `d` siempre se imprimen, incluido el cero.

Ejemplos reales del fichero de salida (`compiler/tests/fixtures/solve_core_oracle.txt`):

- `a=1,b=0,c=-2` (C021): `(0 - sqrt(2))/1, (0 + sqrt(2))/1`.
- `a=1,b=7,c=1` (C025): `(-7 - 3*sqrt(5))/2, (-7 + 3*sqrt(5))/2`. Aquí `D = 45 = 3²·5`, y `g = 1`.
- `a=-1,b=7,c=-1` (C026): `(7 - 3*sqrt(5))/2, (7 + 3*sqrt(5))/2`. Con `a < 0` se invierten los signos de `P` y `d`,
  y el resultado sigue en orden ascendente.

Una raíz doble (`D = 0`) produce una sola raíz racional. Un discriminante cuadrado perfecto produce dos raíces
racionales, ordenadas de menor a mayor.

## 3. Funciones públicas

Los nombres y firmas son los del archivo `compiler/std/solve.ostrin`:

| Función | Firma | Semántica |
| --- | --- | --- |
| `solve_linear` | `(a: Int, b: Int) -> Result<String, String>` | Solución de `a·x + b = 0` como `n/d`. Si `a = 0`: `Ok("all reals")` cuando `b = 0`, y `Ok("no solution")` en otro caso. |
| `solve_quadratic` | `(a: Int, b: Int, c: Int) -> Result<List<String>, String>` | Raíces reales de `a·x² + b·x + c = 0`, ordenadas y distintas. Si `a = 0`, delega en la lineal. |
| `solve_rational_roots` | `(coeffs: List<Int>) -> Result<List<String>, String>` | Raíces racionales de `coeffs[0] + coeffs[1]·x + ...`, como `n/d` ordenadas. Usa `std.polynomial.poly_roots_rational`. |

Las funciones privadas (`abs_int`, `gcd_int`, `within_limit`, `fraction_text`, `surd_text`, `as_fraction`,
`linear_roots` y `bracket`) no forman parte de la API. `fraction_text` y `surd_text` son las que producen el texto
canónico descrito en §2.

## 4. Errores y límites

Los errores son cadenas, y el módulo no aborta. Los textos fijos que produce el código son:

- coeficientes fuera de `|coeff| <= 10^6` en `solve_linear`: `solve_linear: coefficients must satisfy |coeff| <= 1000000`;
- lo mismo en `solve_quadratic`, con el prefijo `solve_quadratic:`;
- discriminante negativo en `solve_quadratic`: `no real roots`;
- en `solve_quadratic` con `a = 0`: `all reals` o `no solution` como `Err` (ver §6);
- `solve_rational_roots` con polinomio cero (lista vacía o todos los coeficientes cero): `zero polynomial`;
- en `solve_rational_roots`, cualquier error de `std.polynomial.poly_roots_rational`, con su texto original
  (por ejemplo `constant or leading coefficient too large for a root search (above 10^12)`).

**El límite `|coeff| <= 10^6` y `Int::MIN`.** `within_limit` compara sin negar el argumento:
`n <= 1000000 and n >= 0 - 1000000`. Si la comparación usara `-n`, el valor `Int::MIN` desbordaría al negarse; con la
forma usada, `Int::MIN` es menor que `-10^6` y se rechaza sin operar sobre él. El límite se aplica solo a `solve_linear`
y `solve_quadratic`. `solve_rational_roots` no aplica esa cota: rige la de `std.polynomial` (10^12) de §4 del documento 31.

El fichero de casos del oráculo no contiene ninguna entrada fuera de `10^6`, así que el rechazo por límite y el de
`Int::MIN` no están cubiertos por el oráculo. Son comprobaciones del código que no tienen un caso de prueba dedicado
en este corte.

## 5. Oráculo independiente

- `scripts/solve-oracle.mjs` implementa las mismas reglas con `BigInt` de Node, sin paquetes externos y sin código
  compartido con el módulo. Su cabecera fija el formato de salida (`id | op | inputs | result`) y las reglas de
  resultado.
- Genera **109 casos**: 69 escritos a mano (lineales, cuadráticos con discriminante cuadrado, irracional, negativo
  y grande, y raíces racionales con ceros, raíces repetidas y polinomios de grado 6) y 40 generados con el generador
  de Park-Miller (semilla 7), con coeficientes en `-30..30`.
- El resultado se guarda en `compiler/tests/fixtures/solve_core_oracle.txt`.
- `examples/solve_core.ostrin` reproduce las mismas 109 filas. Su cabecera indica que los textos de `Err`
  `all reals`, `no solution` y `no real roots` se imprimen como resultados, y que cualquier otro `Err` se imprime con el
  prefijo `error: `.
- La prueba `std_solve_core_matches_oracle_in_both_backends` (en `compiler/tests/examples.rs`) exige que el intérprete
  y el binario nativo produzcan exactamente el fichero de referencia, y que el informe de tipos nativo muestre
  `ast-fallback: 0`.

Dos filas del fichero de referencia son errores impresos como `error: zero polynomial` (C042 y C043). Las filas
`none` (sin raíces racionales) son las que devuelve `solve_rational_roots` con una lista vacía de raíces, por ejemplo
C044 (`[5]`) y C046 (`[-2,0,1]`, cuyas raíces son irracionales).

## 6. La diferencia en `a = 0`

Hay una diferencia que conviene tener presente al leer el código:

- `solve_quadratic` con `a = 0` pasa por `linear_roots`, que convierte `"all reals"` y `"no solution"` en `Err` con el
  mismo texto. Por eso `solve_quadratic(0, 0, 0)` devuelve `Err("all reals")`.
- `solve_linear` con `a = 0` devuelve `Ok("all reals")` o `Ok("no solution")`: son resultados, no errores.
- El oráculo informa de ambos casos como resultados (C001 a C003 para lineales, C013 y C014 para cuadráticos).
- El ejemplo resuelve la diferencia en su capa de presentación: `quadratic_text` imprime como resultado los textos
  `all reals`, `no solution` y `no real roots`; `linear_text` imprime el `Ok` tal cual.

Así, la salida coincide con el oráculo, pero el contrato de la API no es uniforme entre `solve_linear` y
`solve_quadratic`. Un llamador que use `solve_quadratic` debe tratar `Err` con esos tres textos como resultados válidos.

## 7. Rendimiento

No hay medidas de rendimiento de `std.solve`. No se ha hecho ningún benchmark y este documento no afirma tiempos ni
cotas de coste en la práctica. La rama cuadrática hace una factorización por división de prueba del discriminante
(hasta `sqrt(D)`), y `solve_rational_roots` hereda el coste de `poly_roots_rational`; ninguno de esos costes está medido.

## 8. Límites actuales

- Solo una variable, y solo coeficientes enteros en las firmas públicas.
- Solo raíces reales. No hay raíces complejas.
- Sin sistemas de ecuaciones.
- Sin fórmulas cerradas para grado tres o superior, ni siquiera para casos especiales.
- Cota de `10^6` en lineal y cuadrática; cota de `10^12` (de `std.polynomial`) en raíces racionales.
- Diferencia de contrato en `a = 0` entre `solve_linear` y `solve_quadratic` (§6).
- Las comprobaciones de límite (`|coeff| <= 10^6` y `Int::MIN`) no están cubiertas por el oráculo (§4).

## 9. Lo que no está hecho

- Raíces complejas, incluido el caso `D < 0` como par conjugado. Hoy devuelve `Err("no real roots")`.
- Sistemas de ecuaciones lineales o no lineales.
- Fórmulas cerradas para cúbicas y cuartas, y en general para grado tres o superior.
- Aproximación numérica de raíces irracionales de grado mayor que dos.
- Casos de prueba dedicados para el rechazo por límite y para `Int::MIN`.
- Comparación con un motor externo (por ejemplo SymPy): no se ha ejecutado.
- Benchmarks de rendimiento.
