# 24 — Métodos numéricos: `std.numeric` 0.1

Estado: implementado (2026-09-24). Código: `compiler/std/numeric.ostrin`. Ejemplos:
`examples/numeric_methods.ostrin`, `examples/viz_ode.ostrin`, `examples/viz_fft.ostrin`,
`examples/viz_spline.ostrin`, `examples/lab_ode.ostrin`. Web: pestaña ODE del Scientific Lab y tres
figuras de la galería de `viz.html`.

## 1. Objetivo y principios

La fase 6 del plan (matemática científica) necesita los métodos que cualquier curso o laboratorio usa a
diario: raíces, integrales, derivadas, mínimos, interpolación, ecuaciones diferenciales y la
transformada de Fourier. Igual que `std.viz`:

1. **Escrito en Ostrin.** Todo es código Ostrin sobre `Array<Float>` y funciones de primera clase; no
   hay bindings a C ocultos. Así sirve también de prueba de carga para el lenguaje.
2. **Idéntico en todos los backends.** El test diferencial compara la salida del intérprete y la del
   binario nativo para cada ejemplo; la web recalcula los resultados con `ostrinc.wasm`.
3. **Errores como valores.** Los métodos que pueden no converger devuelven `Result<Float, String>`.

## 2. API

```ostrin
import std.numeric

numeric.trapz(xs, ys)                          // regla del trapecio sobre muestras
numeric.simpson(f, a, b, n: 200)               // Simpson compuesta (n se hace par)
numeric.bisect(f, a, b, tol: 1e-12)            // Result<Float, String>, exige cambio de signo
numeric.secant(f, x0, x1)                      // Result<Float, String>
numeric.newton(f, df, x0)                      // Result<Float, String>
numeric.derivative(f, x, h: 1e-5)              // diferencia central
numeric.golden_min(f, a, b)                    // mínimo por sección áurea
numeric.interp(xs, ys, x) / interp_all(xs, ys, at)   // lineal; fuera de los datos extiende los tramos extremos
s = numeric.spline(xs, ys)                     // spline cúbico natural
s.at(x)  s.sample(at)
sol = numeric.rk4(f, y0, t0, t1, steps: 1000)  // f(t, y) -> dy/dt, y: Array<Float>
sol = numeric.rk45(f, y0, t0, t1, tol: 1e-8)   // Dormand–Prince adaptativo
sol.t  sol.y  sol.evaluations  sol.component(i)  sol.final_state()  sol.length()
spec = numeric.fft(signal)                     // Spectrum { re, im }, spec.magnitude()
numeric.ifft(spec)  numeric.frequencies(n, dt)  numeric.amplitude(spec)

// Con unidades: xs: Array<Quantity<X>>, ys: Array<Quantity<Y>>
numeric.unit_trapz(xs, ys)                     // Quantity<X * Y>   (km/h sobre min → km)
numeric.unit_cumtrapz(xs, ys)                  // Array<Quantity<X * Y>>
numeric.unit_gradient(xs, ys)                  // Array<Quantity<Y / X>>
numeric.unit_interp(xs, ys, x)                 // Quantity<Y>; x en cualquier unidad de X
```

Las funciones `unit_*` se escriben con aritmética de cantidades escalares (`xs[i] - xs[i-1]`,
`ys[i] + ys[i-1]`), así que la unidad del resultado sale de las reglas de siempre y no hace falta
ningún caso especial en el runtime. Requieren al menos dos muestras.

## 3. Algoritmos

- **Raíces**: bisección con comprobación de signo; secante y Newton con tolerancia absoluta en el paso
  y límite de iteraciones, que devuelven `Err` si la derivada o la diferencia se anulan.
- **Spline**: sistema tridiagonal del spline natural resuelto con el algoritmo de Thomas; la búsqueda
  del tramo es binaria.
- **RK45**: coeficientes de Dormand–Prince 5(4), error estimado con la diferencia entre órdenes relativa a `1 + max|y|`, factor
  de paso `0.9 (tol/err)^(1/5)` acotado a [0.2, 5]. `Solution.y` guarda un estado por fila.
- **FFT**: Cooley–Tukey radix 2 iterativo con inversión de bits si la longitud es potencia de dos; si
  no, una DFT directa O(n²) con el ángulo reducido módulo n para no perder precisión.
  `amplitude` normaliza a la amplitud de la sinusoide (2|X_k|/n; |X_k|/n para la componente continua y la de Nyquist).

## 4. Hallazgos del compilador

- En C, los argumentos recién creados que se pasaban a clausuras (`f(t, array([...]))`) no se
  liberaban: 2 891 asignaciones vivas en `numeric_methods`. `gen_closure_call` usa ahora la misma ruta
  de propiedad que las llamadas directas; quedan 38.
- La emisión HIR de `obj.campo = valor` no retenía el valor nuevo ni liberaba el viejo. Al liberar los
  argumentos temporales de los métodos, `fig.describe("..." + texto)` dejaba el campo apuntando a
  memoria liberada (ASan: heap-use-after-free en `render`). Ahora retiene y libera como la ruta AST.

Para escribirlas hizo falta que los genéricos de dimensión se infieran dentro de contenedores:
`fn f<X: Dimension>(xs: Array<Quantity<X>>) -> Quantity<X>` dejaba `X` sin sustituir en el tipo de
retorno (y `f(t) as min` fallaba con E1026). El checker recorre ahora `Array`, `List`, `Set`, `Map`
y tipos función para enlazar `X`, y una segunda aparición con otra dimensión es E1042
(`examples/unit_generic_errors.ostrin`).

## 5. Límites y siguientes pasos

| Pendiente | Nota |
|---|---|
| métodos implícitos (BDF, Rosenbrock) | necesarios para sistemas rígidos |
| eventos y salida densa en ODE | detener la integración en un cruce; interpolar entre pasos |
| cuadratura adaptativa, integrales múltiples | Gauss–Kronrod |
| optimización multivariable | Nelder–Mead, BFGS |
| unidades en ODEs y raíces | `unit_*` cubre integrales, derivadas e interpolación; `rk45` y `newton` siguen en `Float` (un estado con dimensiones mezcladas necesitaría tuplas o records de cantidades) |
