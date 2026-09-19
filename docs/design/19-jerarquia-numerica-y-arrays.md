# 19. Jerarquía numérica y arrays

*Estado: propuesta de especificación. Hoy solo existen `Int` (64 bits con signo), `Float` (64 bits) y `List<T>`.*

## 1. Principios

1. **Sin conversiones silenciosas con pérdida.** Mezclar anchos, o `Int` con `Float`, requiere conversión explícita salvo ampliaciones exactas.
2. **Overflow definido**, nunca indefinido: por defecto comprobado (error en ejecución), envoltura con `wrapping_*`, saturación con `saturating_*`.
3. **Las unidades y las dimensiones son ortogonales al tipo numérico**: `Quantity<D, T>` con `T` numérico (por defecto `Float64`).
4. Las reglas deben poder comprobarse en el checker y ser idénticas en intérprete y nativo.

## 2. Tipos escalares

| Familia | Tipos | Notas |
|---|---|---|
| Enteros con signo | `Int8 Int16 Int32 Int64 Int128` | `Int` = alias de `Int64` |
| Enteros sin signo | `UInt8 UInt16 UInt32 UInt64 UInt128` | |
| Reales | `Float16 BFloat16 Float32 Float64` | `Float` = alias de `Float64`; `Float128` cuando el destino lo soporte |
| Precisión arbitraria | `BigInt`, `Decimal`, `Rational` | Biblioteca estándar |
| Complejos | `Complex<T>` | `T` real |
| Intervalos | `Interval<T>` | Aritmética de intervalos (base de la incertidumbre) |

Literales: `1` es `Int` por defecto; `1u8`, `2.5f32`, `3+4i` con sufijos explícitos. Un literal sin sufijo adopta el tipo esperado si cabe exactamente (`x: UInt8 = 200` es válido, `= 300` es error de compilación).

## 3. Conversiones

- **Ampliación exacta implícita**: `Int8→Int16→Int32→Int64`, `UInt8→UInt16→…`, `UIntN→Int(2N)`, `Float32→Float64`. Nada más.
- **Explícita**: `x as Float32` (con posible pérdida), `x.try_into<UInt8>()` devuelve `Result`.
- `Int → Float` **nunca** es implícita (puede perder precisión a partir de 2⁵³).

## 4. Aritmética y unidades

- `Quantity<D, T>` conserva la regla actual (dimensión estática, unidad en ejecución) y pasa a aceptar `T`.
- `Int / Int` sigue siendo división entera (decisión ya tomada); `Float / Float` es real; `Int / Float` requiere conversión explícita.
- Incertidumbre: `Measured<T>` = valor + desviación con propagación de errores; `Interval<T>` para cotas rigurosas. Ambos componen con `Quantity`.

## 5. Arrays, matrices y tensores

Tipo base: `Array<T, Shape>`, donde `Shape` es una tupla de dimensiones **estáticas o dinámicas**.

```text
Vector<T, N>        = Array<T, (N,)>
Matrix<T, R, C>     = Array<T, (R, C)>
Tensor<T, D1..Dk>   = Array<T, (D1..Dk)>
```

- Las dimensiones estáticas (`1024`) permiten comprobar formas en compilación (`A @ B` exige que las columnas de `A` igualen las filas de `B`) y desenrollar/vectorizar; las dinámicas se comprueban en ejecución.
- **Layout** row‑major por defecto con `strides` explícitos; las vistas (`slice`, `transpose`, `reshape`) no copian: son préstamos internos al runtime (ver documento 18).
- **Operaciones**: aritmética elemento a elemento con **broadcasting** (reglas de NumPy, comprobadas estáticamente cuando es posible), reducciones (`sum`, `mean`, `min`, `max`, por eje), `@` para el producto matricial, `.T`, y comparaciones que devuelven `Array<Bool, _>`.
- **Sintaxis**: los literales `[1, 2, 3]` siguen siendo `List`; `array([1, 2, 3])` y `linspace(a, b, n)` construyen `Array`. Rangos indexables `a[1..4]`, `a[:, 0]`.
- Los elementos pueden ser `Quantity`: `Vector<Quantity<Length>, 3>` es válido y `dot` produce `Quantity<Length^2>`.

## 6. Implementación por fases

1. **Tipos enteros de ancho fijo** (checker → intérprete → nativo con `stdint`), con literales y conversiones. Es el cambio más contenido y desbloquea el resto.
2. **`Float32`** y conversiones; `Complex`.
3. **`Array<T, Shape>` dinámico** (forma en ejecución) con aritmética, broadcasting y reducciones en el runtime C.
4. **Formas estáticas** y verificación en el checker.
5. **Álgebra lineal** (LU, QR, SVD, `solve`) sobre un backend intercambiable (BLAS/LAPACK opcional, implementación propia de referencia).
6. `Measured` e `Interval`.

## 7. Pruebas

Cada fase añade: ejemplos en `examples/`, comparación intérprete↔nativo (ya automática), casos `*_errors.ostrin` para cada regla de conversión y de forma, y *benchmarks* frente a C y NumPy para las operaciones de rendimiento.
