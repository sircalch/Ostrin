# 25 — Mediciones e incertidumbre

**Estado:** propuesta de diseño; no forma parte todavía de la sintaxis estable ni de la
biblioteca estándar publicada.

**Fecha:** 2026-09-29

## 1. Propósito

Ostrin ya puede expresar cantidades físicas con dimensiones y unidades. El siguiente paso
científico es representar el conocimiento limitado que acompaña a un valor medido sin convertir
una cifra con apariencia científica en una afirmación de precisión que el programa no puede
justificar.

La hipótesis de este documento es que una medición debe conservar, de forma explícita y
componible:

- el valor nominal;
- el modelo de incertidumbre y sus supuestos;
- la correlación con otras mediciones derivadas de las mismas fuentes;
- la dimensión física cuando el valor es una `Quantity`;
- la procedencia opcional que permite explicar cómo se obtuvo.

Este documento fija interfaces y contratos para una futura implementación. No declara que
`Measurement<T>` exista hoy.

## 2. Revisión de antecedentes

La propuesta parte de fuentes que resuelven problemas distintos:

| Fuente | Decisión que informa el diseño |
| --- | --- |
| [JCGM 100:2008 / GUM](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=935265) y la [introducción de NIST](https://physics.nist.gov/cuu/Uncertainty/international2.html) | Separar evaluación de incertidumbre, incertidumbre estándar, covarianza y forma de expresión del resultado. |
| [NIST NeXLUncertainties.jl](https://pages.nist.gov/NeXLUncertainties.jl/gettingstarted/) | Mantener una ruta linealizada para el caso común y reservar Monte Carlo para modelos que lo necesiten; relacionar el diseño con GUM Supplement 1. |
| [Measurements.jl](https://github.com/JuliaPhysics/Measurements.jl/blob/main/docs/src/usage.md) | Conservar la identidad de las fuentes independientes y la dependencia de los resultados para que cancelaciones como `x - x` sean exactas. |
| [uncertainties de Python](https://pythonhosted.org/uncertainties/user_guide.html) | Exponer derivadas, matrices de covarianza y operaciones sobre arrays sin fingir que dos valores con la misma desviación son la misma medición. |

Estas referencias no se interpretan como una prueba de novedad. Una futura publicación tendría
que hacer una revisión bibliográfica completa y demostrar qué integración de Ostrin es nueva.

## 3. Objetivos

1. Hacer que el valor y su incertidumbre viajen juntos en operaciones aritméticas y científicas.
2. Preservar correlaciones de forma que el resultado dependa de las fuentes reales y no solo de
   dos números `sigma` aislados.
3. Integrarse con `Quantity<Dim>` sin perder la dimensión de la incertidumbre.
4. Distinguir una magnitud exacta, una medición con incertidumbre conocida y una incertidumbre
   desconocida.
5. Permitir una ruta determinista y reproducible para el caso linealizado.
6. Exportar tablas, figuras y metadatos con suficiente información para interpretar el resultado.
7. Mantener paridad entre intérprete, nativo y WASM antes de anunciar la capacidad.

## 4. No objetivos de la primera versión

- No resolver automáticamente todos los modelos estadísticos o probabilísticos.
- No convertir toda operación con `Float` en una operación de medición.
- No tratar una cota de intervalo como si fuera una desviación estándar.
- No calcular intervalos de cobertura sin conocer el modelo y el nivel solicitado.
- No incluir timestamps, identificadores de máquina o servicios externos en el valor escalar por
  defecto.
- No prometer propagación correcta de segundo orden para funciones fuertemente no lineales en la
  primera versión.

## 5. Modelo conceptual

La forma conceptual mínima es:

```text
Measurement<T> = {
    value: T,
    uncertainty: UncertaintyModel<T>,
    provenance: Optional<MeasurementProvenance>,
}
```

El nombre y la sintaxis son provisionales. La representación final puede ser una estructura
especializada del compilador, siempre que conserve el mismo contrato observable.

### 5.1 Valor y unidad

Para una magnitud física, el tipo esperado es:

```text
Measurement<Quantity<Length>>
Measurement<Quantity<Temperature>>
```

La incertidumbre debe tener la misma dimensión que el valor. `2 m ± 0.1 s` debe ser un error de
tipos. Las operaciones combinan dimensiones con las reglas existentes de `Quantity` y combinan
la incertidumbre con la regla matemática correspondiente.

### 5.2 Estados de conocimiento

El diseño distingue explícitamente tres estados:

```text
Exact(value)
Standard(value, sigma, degrees_of_freedom?)
Unknown(value, reason?)
```

`Standard` representa una incertidumbre estándar, no un intervalo de confianza. Un `sigma = 0`
solo significa exactitud cuando la construcción lo declara; la ausencia de información no se
convertirá silenciosamente en cero.

Versiones posteriores pueden añadir `Interval`, `Distribution` y modelos de cobertura, pero no
se mezclarán con `Standard` sin una conversión explícita.

### 5.3 Fuentes y correlación

Cada medición independiente obtiene una identidad de fuente. Los valores derivados conservan un
mapa de sensibilidades o una representación equivalente:

```text
x = source("caliper", 12.30 mm, sigma = 0.02 mm)
y = 2 * x
```

La incertidumbre de `y` conoce que depende de la misma fuente que `x`. Por eso `x - x` puede ser
exacto, mientras que dos llamadas independientes a `source` siguen siendo independientes aunque
tengan el mismo valor nominal y la misma desviación.

Para un vector de fuentes con covarianza `Σ` y gradiente `J`, la primera propagación propuesta es:

```text
u² = J · Σ · Jᵀ
```

El resultado debe conservar suficiente información para calcular componentes de incertidumbre,
covarianzas y contribuciones dominantes sin guardar una copia completa del historial de cada
expresión.

### 5.4 Procedencia

La procedencia no es la incertidumbre. Puede registrar opcionalmente:

- etiqueta de fuente o instrumento;
- método de evaluación;
- unidad y conversión aplicada;
- identificador del conjunto de datos;
- versión del algoritmo;
- parámetros de propagación;
- semilla cuando se use Monte Carlo.

La procedencia debe ser un metadato compartido o de artefacto. No debe hacer que cada `Float`
lleve una estructura pesada cuando el usuario no la solicita.

## 6. Operaciones y reglas

### 6.1 Operaciones algebraicas

La primera implementación debe cubrir suma, resta, producto, cociente, potencias enteras y las
conversiones de unidades ya soportadas. Para funciones diferenciables, el camino linealizado
usa las derivadas disponibles y conserva las dependencias de fuente.

Las funciones trascendentes requieren un argumento adimensional según las reglas de `Quantity`.
El resultado conserva la unidad y la sensibilidad correctas cuando la operación es válida.

### 6.2 Funciones sin derivada disponible

El compilador no debe inventar una desviación. Según el caso, una llamada debe:

1. usar una regla analítica registrada;
2. exigir una función de sensibilidad suministrada por el usuario;
3. cambiar explícitamente a un método numérico o Monte Carlo;
4. producir un diagnóstico de incertidumbre no soportada.

El diagnóstico debe identificar la función y el modelo que falta.

### 6.3 Comparación

La igualdad estructural de mediciones y la compatibilidad estadística son preguntas distintas.
La primera versión no debe convertir `a == b` en una prueba de hipótesis escondida. Se reservan
operaciones explícitas, por ejemplo `same_source`, `compatible` o `within`, con un nivel o una
tolerancia documentados.

### 6.4 Arrays y matrices

`Array<Measurement<T>>` puede ser la primera superficie de arrays. La API debe ofrecer después:

- valores nominales;
- incertidumbres por elemento;
- matriz de covarianza cuando el modelo la requiera;
- reducciones que respeten correlaciones;
- vistas sin copiar metadatos innecesariamente.

Una reducción no puede sumar desviaciones como si todos los elementos fueran independientes si
el grafo de fuentes demuestra lo contrario.

## 7. Estrategias de propagación

### Fase linealizada

Es el camino predeterminado para operaciones suaves y pequeñas incertidumbres. Debe publicar sus
supuestos, sus derivadas y cualquier advertencia de no linealidad. Los resultados pueden exponer:

```text
result.value()
result.uncertainty()
result.contributions()
```

### Fase Monte Carlo

Una API explícita debe aceptar un modelo de distribución, número de muestras, semilla y política
de resumen. La semilla pertenece a la reproducibilidad y debe aparecer en la procedencia. Esta
fase seguirá los principios de JCGM 101/102 y no se reducirá a sumar desviaciones independientes.

### Fase de cobertura

Los intervalos de cobertura y los grados de libertad se añadirán cuando exista una representación
clara de distribución y una API para declarar el nivel deseado. `sigma` por sí sola no autoriza a
afirmar “95 %”.

## 8. Interacción con el compilador

El checker debe:

- rechazar operaciones entre incertidumbres con dimensiones incompatibles;
- impedir la conversión implícita de `Measurement<T>` a `T`;
- exigir `value()` o una operación equivalente para descartar la incertidumbre;
- conservar el parámetro `T` durante la resolución de genéricos;
- distinguir funciones puras de funciones que consumen una semilla o datos externos;
- informar cuando una operación necesita un modelo de propagación no disponible.

La información de tipo científico debe llegar al HIR y al IR antes de añadir optimizaciones. La
representación runtime puede borrarse cuando un valor es `Exact` y el backend demuestra que no hay
metadatos observables que conservar.

## 9. Representación nativa y WASM

La propuesta inicial usa una representación separada del valor nominal:

```text
MeasurementScalar<T> = {
    value: T,
    uncertainty_state: handle,
}
```

El `handle` identifica un nodo de sensibilidad o un bloque compacto de covarianza. Para valores
exactos puede ser un estado inmediato. Para arrays, los valores y la estructura de incertidumbre
pueden almacenarse en buffers separados para no penalizar recorridos que solo consumen la parte
nominal.

El intérprete, C nativo y WASM deben producir la misma salida numérica, el mismo estado de error y
la misma procedencia serializada, con tolerancias explícitas para operaciones flotantes.

## 10. Visualización y publicación

Cuando la semántica exista, `std.viz` podrá consumirla para:

- barras de error y bandas de confianza declaradas;
- tooltips con valor, incertidumbre y unidad;
- tablas con columnas nominales y de incertidumbre;
- exportaciones SVG/HTML/PDF con procedencia;
- leyendas que indiquen el modelo y el nivel de cobertura.

Las figuras no deben rotular una banda como “confianza” si solo recibieron `sigma`. La figura debe
llevar el método, los supuestos, la semilla cuando aplique y el hash de los datos según el contrato
de procedencia de [`23-motor-de-visualizacion.md`](23-motor-de-visualizacion.md).

## 11. Plan de implementación

1. Revisar este documento contra `Quantity`, arrays y el modelo de efectos.
2. Implementar `Exact` y `Standard` para escalares en el intérprete con operaciones básicas.
3. Añadir fuentes correlacionadas y pruebas de cancelación.
4. Integrar `Measurement<Quantity<Dim>>` y conversiones de unidades.
5. Llevar la representación al HIR/IR y comprobar ownership en nativo.
6. Añadir arrays, covarianza y salida de contribuciones.
7. Añadir propagación Monte Carlo explícita y semilla reproducible.
8. Integrar error bars/bands con procedencia en `std.viz`.
9. Habilitar WASM y la galería únicamente cuando las tres rutas tengan paridad.

## 12. Evidencia y pruebas necesarias

- Casos positivos de `Measurement<Float>` y `Measurement<Quantity<Dim>>`.
- Errores por dimensiones incompatibles, incertidumbre descartada implícitamente y funciones sin
  modelo.
- Identidades correlacionadas como `x - x` y `x / x`.
- Comparación contra fórmulas analíticas y casos de covarianza conocida.
- Diferencial intérprete/nativo/WASM con tolerancia declarada.
- Leak-check, ASan y UBSan para sensibilidad, covarianza y arrays.
- Property-based tests para simetría y semidefinitud de covarianzas válidas.
- Pruebas de procedencia y exportación de figuras.
- Benchmarks separados para valores exactos y mediciones con metadatos.

## 13. Preguntas abiertas

1. ¿La covarianza debe vivir en un grafo global por evaluación o en bloques locales persistentes?
2. ¿Qué política de grados de libertad debe usar `combine`?
3. ¿Cómo se representa una distribución no gaussiana sin hacerla parte del núcleo escalar?
4. ¿Cómo se diagnostica una linealización inadecuada de forma útil y determinista?
5. ¿Qué serialización permite reproducir resultados sin exponer identificadores privados?
6. ¿Qué garantías de cancelación pueden demostrarse cuando el backend optimiza el grafo?
7. ¿Qué parte debe pertenecer al lenguaje y qué parte a un paquete científico versionado?

## 14. Decisiones que no se deben tomar todavía

- No fijar un operador `±` como sintaxis principal antes de evaluar su interacción con precedencia,
  unidades y fuentes independientes.
- No prometer “incertidumbre correcta” para funciones que no declaran derivadas o distribución.
- No tratar el modelo de primer orden como válido para todos los problemas.
- No afirmar novedad académica por integrar ideas ya presentes en GUM, Measurements.jl o
  uncertainties.

