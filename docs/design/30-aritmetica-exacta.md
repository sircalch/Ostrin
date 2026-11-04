# 30 · Aritmética exacta (`std.bigint`)

Estado: **primer corte implementado**. Enteros de precisión arbitraria escritos en Ostrin, con la misma
salida en el intérprete, el backend nativo y WASI. Es la base de la aritmética exacta que necesita el
motor simbólico (documento 29), cuyos racionales hoy están limitados a 64 bits.

## 1. Decisión: implementación propia en Ostrin, no GMP

La directiva pide no escribir multiprecisión de bajo nivel si existe una biblioteca madura y apropiada.
Se evaluó y se descartó incrustar GMP o mini-gmp por tres razones concretas:

- **Licencia.** Ostrin es MIT y el runtime viaja dentro de cada ejecutable generado. GMP es LGPL/GPL, y
  `mini-gmp` es de doble licencia LGPLv3+/GPLv2+: distribuirlos dentro de binarios y módulos WASI
  generados impondría obligaciones a los programas de los usuarios.
- **Un único comportamiento en tres backends.** El mismo código fuente debe ejecutarse en el intérprete,
  en C nativo y en WASI con resultados idénticos bit a bit. Una dependencia nativa obligaría a mantener
  una segunda implementación en el intérprete (por ejemplo `num-bigint` en Rust) y a probar su equivalencia.
- **Toolchain.** El pipeline nativo compila un único archivo C con GCC; añadir una biblioteca externa
  complicaría la compilación a WASI y a las plataformas Windows/macOS.

El coste es rendimiento: ver §4. Si una carga real lo exige, un backend opcional de GMP puede añadirse
después detrás de la misma API, sin cambiar la semántica pública.

## 2. Representación y semántica

`BigInt { negative: Bool, limbs: List<Int> }`: signo y bloques (*limbs*) little-endian en base 10⁹, sin ceros a la
izquierda; el cero es la lista vacía y nunca es negativo. Cada producto parcial cabe en `Int` (10¹⁸ <
9,2·10¹⁸).

| Operación | Semántica |
| --- | --- |
| `add`, `sub`, `mul`, `negate`, `abs_value`, `pow` | exactas, sin desbordamiento |
| `divmod` | división truncada como `Int`: el cociente se redondea hacia cero y el resto toma el signo del dividendo, de modo que `a = q·b + r`; `b = 0` devuelve error |
| `gcd` | no negativo; `gcd(0, 0) = 0` |
| `compare`, `equals`, `sign`, `is_zero` | orden total |
| `parse`, `to_string` | decimal con signo opcional; errores tipados ante dígitos inválidos o texto vacío |
| `to_int` | `Err` si no cabe en 64 bits (nunca trunca) |
| `to_float` | redondeo bloque a bloque; puede perder los dígitos bajos de valores grandes |
| `factorial` | `n!` para `n ≥ 0` |

## 3. Validación

- 40 operandos con signo de hasta 60 dígitos generados por un Park–Miller determinista; suma, resta,
  producto, cociente, resto y gcd coinciden **fila a fila** con un oráculo independiente (el `BigInt` nativo de
  JavaScript, con el mismo generador).
- Identidades comprobadas en el propio programa para los 40 casos: `a = q·b + r`, `|r| < |b|`, signo del
  resto, `gcd | a`, `gcd | b` y ida y vuelta de `to_string`/`parse`.
- Constantes conocidas: 2¹⁰⁰, 30!, división con signo mixto, gcd(20!, 2⁷⁰) = 2¹⁸, `i64::MIN` y la
  frontera de `to_int`.
- Paridad intérprete/nativo, cero fallbacks al AST (`ir=34`) y cero asignaciones vivas.

## 4. Rendimiento medido

Nativo, Windows x64, compilación con el C de Ostrin: factorial(2000) (5736 dígitos), 3¹⁰⁰⁰⁰ (4772 dígitos),
su división y el gcd de ambos tardan **≈0,37 s**. Una primera versión con búsqueda binaria de 30 pasos por
bloque tardaba 240 s en la misma carga (≈650× más lenta); el cuello de botella era repetir esa búsqueda
incluso cuando el dígito del cociente es 0. La versión actual estima el dígito con los tres primeros
bloques en coma flotante y lo corrige exactamente. El intérprete es considerablemente más lento. El
algoritmo sigue siendo escolar: multiplicación y división cuadráticas. No hay Karatsuba, Newton ni FFT; no es
apto para tamaños criptográficos.

## 5. Racionales exactos y motor simbólico

`std.bigint` expone también racionales exactos como texto decimal reducido (`"N"` o `"N/D"`):
`rational_make`, `rational_add`, `rational_mul`, `rational_reciprocal`, `rational_sign`,
`rational_to_float` y conversiones a `Int` cuando caben. Se usa texto porque otros módulos de la
biblioteca estándar no pueden nombrar `bigint.BigInt` en una firma. `std.symbolic` los usa para pasar
a precisión arbitraria solo cuando un resultado exacto deja de caber en 64 bits (documento 29 §3).

## 6. Límites actuales

No hay `BigFloat` (multiprecisión de coma flotante) ni un tipo `Rational` tipado todavía, ni conversiones
automáticas con `Int` (`from_int`/`to_int` explícitas), ni operadores `+ - * /` sobrecargados. Los
racionales en texto reparsean en cada operación, lo que es correcto pero lento para cargas intensivas; un tipo
`Rational` con `BigInt` interno es el siguiente paso.
