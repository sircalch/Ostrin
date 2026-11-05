# 31 · Polinomios y ecuaciones (`std.polynomial`)

Estado: **primer corte implementado** (`std.polynomial`, PR #267). Polinomios univariados con coeficientes
racionales exactos sobre `std.bigint` (documento 30): suma, resta, producto, división con resto, máximo común
divisor, evaluación, derivada, raíces racionales y factorización en factores lineales. El mismo código se ejecuta
en el intérprete y en el backend nativo con la misma salida.

`std.solve` (resolución exacta de ecuaciones lineales, cuadráticas y de raíces racionales) está **en curso** en
otra rama. No forma parte de este corte y este documento no la describe como terminada.

## 1. Alcance y decisión

El módulo es deliberadamente pequeño: polinomios de una variable, con coeficientes racionales, donde cada paso es
exacto. No hay coma flotante en ningún punto del cálculo, de modo que no hay redondeo ni desbordamiento. El coste
asumido es que cada operación sobre coeficientes parsea y reimprime sus operandos en texto racional. Es aceptable
para los grados y tamaños de coeficiente previstos y se documenta como límite en §6.

El módulo no conoce el tipo `Rational` de `std.bigint` en sus firmas: otros módulos de la biblioteca estándar no
pueden nombrar `bigint.Rational` en una firma, así que los coeficientes viajan como texto canónico (§2) y se
convierten con `bigint.fraction_parse` y `bigint.fraction_to_string` dentro de cada operación.

## 2. Representación

```
pub record Poly { coeffs: List<String> }
```

- `coeffs[i]` es el coeficiente de `x^i`: el orden es de grado ascendente.
- Cada coeficiente es un racional en la forma canónica de `std.bigint`: `"N"` o `"N/D"`, en términos
  irreducibles, con `D > 1` y el signo en `N`. Por eso dos polinomios iguales tienen registros iguales.
- El polinomio cero no tiene coeficientes (`coeffs` vacío). Todo polinomio no cero tiene coeficiente líder distinto
  de cero: las operaciones recortan los ceros finales (`trim`).
- `poly_degree` devuelve `-1` para el polinomio cero.
- `poly_to_string` imprime en orden ascendente, por ejemplo `[1, 0, -3/2]`; el cero se imprime como `0`.

Los registros auxiliares son `Division { quotient, remainder }` y
`Factorization { factors: List<Poly>, cofactor: Poly }`.

## 3. Operaciones y semántica

Las firmas son las del archivo `compiler/std/polynomial.ostrin`. Las operaciones que devuelven `Result` no abortan;
los errores son cadenas.

| Función | Firma | Semántica |
| --- | --- | --- |
| `poly_from_ints` | `(List<Int>) -> Poly` | Coeficientes enteros en orden ascendente; recorta ceros finales. |
| `poly_from_texts` | `(List<String>) -> Result<Poly, String>` | Coeficientes `"N"` o `"N/D"`; `Err` si un texto no es racional válido (por ejemplo `"1/0"`). |
| `poly_degree` | `(Poly) -> Int` | Grado; `-1` para el cero. |
| `poly_to_string` | `(Poly) -> String` | Texto ascendente entre corchetes; `"0"` para el cero. |
| `poly_add` | `(Poly, Poly) -> Result<Poly, String>` | Suma exacta coeficiente a coeficiente. |
| `poly_sub` | `(Poly, Poly) -> Result<Poly, String>` | Resta exacta. |
| `poly_mul` | `(Poly, Poly) -> Result<Poly, String>` | Producto por convolución exacta; el cero si un operando es cero. |
| `poly_divmod` | `(Poly, Poly) -> Result<Division, String>` | División sobre los racionales: `a = q·b + r` con `grado(r) < grado(b)`. Si `grado(a) < grado(b)`, el cociente es cero y el resto es `a`. `Err` si `b` es el cero. |
| `poly_gcd` | `(Poly, Poly) -> Result<Poly, String>` | Máximo común divisor por algoritmo de Euclides, normalizado a mónico. `gcd(0, 0)` es el cero. |
| `poly_eval` | `(Poly, String) -> Result<String, String>` | `p(x)` por la regla de Horner, con `x` racional en texto; el resultado es texto canónico. Para el cero da `"0"`. |
| `poly_derivative` | `(Poly) -> Result<Poly, String>` | Derivada formal; la derivada de una constante es el cero. |
| `poly_roots_rational` | `(Poly) -> Result<List<String>, String>` | Raíces racionales distintas en orden ascendente, con coeficientes racionales permitidos. `Err` para el cero y fuera de los límites de §4. |
| `poly_factor_linear` | `(Poly) -> Result<Factorization, String>` | Divide por cada raíz racional tantas veces como divida. Los factores son `(x - r)` en orden ascendente de `r`; el cofactor conserva el coeficiente líder y cualquier parte sin raíces racionales. |

Invariante de `poly_factor_linear`: el polinomio original es igual al cofactor multiplicado por el producto de los
factores. El ejemplo `polynomial_core.ostrin` lo comprueba en cada caso (fila `check`).

Errores con texto fijo que el código produce:

- división por el polinomio cero: `polynomial division by the zero polynomial`;
- raíces del cero: `the zero polynomial has every rational root`.

## 4. Raíces racionales y límites numéricos

`poly_roots_rational` aplica el teorema de las raíces racionales sobre el polinomio multiplicado por el mínimo común
múltiplo de los denominadores (`clear_denominators`). Después prueba cada candidato `±p/q` con `p` divisor del
término constante y `q` divisor del coeficiente líder, con `gcd(p, q) = 1`. Antes de buscar, separa la raíz cero
(`x^k` factor) y la añade si corresponde.

Para que la búsqueda sea finita y barata, hay tres cotas, todas de `10^12`:

- el valor absoluto del término constante y del coeficiente líder del polinomio entero (sin el factor `x^k`);
- el mínimo común múltiplo de los denominadores de los coeficientes originales.

Fuera de esas cotas `poly_roots_rational` devuelve `Err` con uno de estos mensajes: `rational coefficient too large
for a root search`, `coefficient denominators too large for a root search (lcm above 10^12)` o `constant or leading
coefficient too large for a root search (above 10^12)`. Los divisores se calculan por división de prueba hasta
la raíz cuadrada del número, de modo que el coste es del orden de `10^6` pasos por cota.

## 5. Validación

- `examples/polynomial_core.ostrin` genera 40 polinomios enteros con un generador lineal de 31 bits (semilla
  fija). Cada uno pasa por todas las operaciones públicas de §3.
- `scripts/polynomial-oracle.mjs` regenera los mismos casos con aritmética `BigInt` de Node, sin código compartido
  con el módulo, e imprime las mismas filas. El resultado se guarda en `compiler/tests/fixtures/polynomial_core_oracle.txt`.
- La prueba `std_polynomial_core_matches_node_oracle_in_both_backends` (en `compiler/tests/examples.rs`) exige que
  el intérprete y el binario nativo produzcan exactamente ese fichero, y que el informe de tipos nativo muestre
  `ast-fallback: 0`.
- Casos de borde fijos en el mismo ejemplo: raíces de `2/3 + x/3`, texto racional inválido (`1/0`), un término
  constante de `10^13` que debe rechazarse en la búsqueda de raíces, el cero sin raíces, la derivada de una
  constante y el gcd con coeficientes racionales.
- Cada caso también comprueba la división por el polinomio cero (fila `divzero`) y la identidad de la factorización
  (fila `check`).

## 6. Rendimiento

No hay medidas de rendimiento de `std.polynomial`. No se ha hecho ningún benchmark y este documento no afirma
tiempos. El diseño (coeficientes en texto, parseo en cada operación, búsqueda de raíces por división de prueba)
sugiere que es lento para grados o coeficientes grandes; eso no está medido.

## 7. Límites actuales

- Solo una variable. No hay polinomios multivariados.
- Solo raíces racionales. No hay aislamiento de raíces complejas ni irracionales, ni aproximación numérica de raíces.
- La factorización sobre los enteros se limita a factores lineales racionales. No se factoriza sobre `Z` más allá de eso.
- Cotas de `10^12` en la búsqueda de raíces (§4).
- Coeficientes en texto: correcto, pero cada operación reparsea sus operandos.

## 8. Lo que no está hecho

- `std.solve` (ecuaciones lineales, cuadráticas y raíces racionales como API pública): en curso en otra rama.
- Comparación con un motor externo (por ejemplo SymPy): no se ha ejecutado.
- Factorización sobre los enteros más allá de factores lineales.
- Benchmarks de rendimiento.
