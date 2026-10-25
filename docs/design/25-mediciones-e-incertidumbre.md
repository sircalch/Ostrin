# 25 — Mediciones e incertidumbre

**Estado:** propuesta en implementación experimental; `std.measurements` ofrece un API escalar
funcional, una covarianza densa acotada para fuentes escalares adimensionales y una proyección de
series `List<Measurement<Float>>` para valores e incertidumbres.
`Measurement<Quantity<D>>` tiene pruebas experimentales de paridad de salida en intérprete y nativo
para construcción, suma/resta correlacionada, productos y propagación de incertidumbre estándar.
El smoke, el laboratorio y el fixture de superficie directa generan `main` mediante IR y reportan
cero fallbacks AST y cero divergencias de tipos en los casos medidos. El fixture directo también
compara la salida nativa con el intérprete, verifica `live_allocations=0` y emite C para WASI. Esto
acredita esos programas, no el soporte general de `Measurement<Quantity<D>>` ni la corrección
universal de ownership. `Array<Measurement<Float>>` tiene ahora un perfil estructural experimental
de una dimensión para construcción, indexación, `get`, slicing, selección por máscara booleana,
conversión a lista y visualización; cada límite probado conserva y libera los elementos. Las
operaciones numéricas sobre arrays, covarianza de valores dimensionales o arrays, y Monte Carlo
siguen pendientes.

**Fecha:** 2026-10-24

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

Este documento fija interfaces y contratos para una implementación gradual. La primera fase
ejecutable vive en `compiler/std/measurements.ostrin`: conserva fuentes de sensibilidad explícitas
para `Measurement<Float>`, propaga suma/resta/producto/cociente y representa incertidumbre
desconocida sin convertirla en cero. `exact` y `standard` rechazan valores nominales y sigmas no
finitos mediante el builtin tipado `is_finite` para `Float`/`Float32`; las sensibilidades y la varianza
también se validan al consultar la incertidumbre. La superficie sigue siendo experimental y todavía
no declara semántica de operadores ni integración nativa completa con `Quantity<T>`. Para
`Measurement<Quantity<D>>`, el smoke y `measurement_lab.ostrin` registran salida coincidente en
intérprete y nativo, emisión C para WASI en el smoke, `live_allocations=0`, cero fallbacks AST y cero
divergencias de tipos; `main` se genera mediante IR en ambos reportes. Estas pruebas acreditan solo
los casos ejecutados, no soporte nativo general ni corrección universal de ownership. La ruta directa
que instancia `div_quantity` para dimensiones iguales aún falla durante el lowering nativo antes de
ejecutar la rama que debería devolver `Err`.

### 1.1 Evidencia de las fases escalar y dimensional experimental

`examples/measurement_scalar.ostrin` cubre construcciones `exact`, `standard` y `unknown`,
validación de finitud, sigma y etiquetas, combinación de fuentes independientes, cancelación
correlacionada (`x - x = 0 ± 0`), propagación analítica de `x*x` y `x*y`, y paridad intérprete/native.
`examples/measurement_record.ostrin` añade almacenamiento y reemplazo de
`Measurement<Float>` dentro de un record, con paridad intérprete/native, C para
WASI y `live_allocations=0`; la prueba
`std_measurements_scalar_record_field_preserves_ownership` verifica además
que el destructor del record libera la medición exactamente una vez.
`examples/measurement_return.ostrin` cubre el paso como argumento y el retorno
de `Measurement<Float>` con la misma paridad y comprobación de ownership.
`examples/measurement_clone.ostrin` cubre la copia explícita mediante `clone`;
los alias implícitos y los payloads compuestos continúan fuera del contrato.
`examples/measurement_closure.ostrin` cubre la captura por closure y la
destrucción del entorno sin perder la medición capturada.
`examples/measurement_list.ostrin` cubre una colección de
`Measurement<Float>` construida por el usuario, acceso indexado y destrucción,
con paridad intérprete/native, emisión WASI y `live_allocations=0`.
`examples/measurement_branch.ostrin` cubre la selección de una medición desde
las dos ramas de una función, con la misma evidencia de paridad y ownership.
`examples/measurement_loop.ostrin` cubre reemplazos repetidos dentro de un
loop y la liberación del valor final.
`examples/measurement_phi.ostrin` cubre la selección de dos mediciones en un
`Phi` explícito del backend nativo.
`examples/measurement_result.ostrin` cubre `Result<Measurement<Float>, String>`
en las ramas `Ok` y `Err`, con `match`, emisión WASI y `live_allocations=0`.
`examples/measurement_task.ostrin` cubre captura diferida, `join` y destrucción
del entorno de una tarea para `Measurement<Float>`.
`examples/measurement_array_shape.ostrin` cubre el perfil estructural unidimensional de
`Array<Measurement<Float>>`: construcción desde lista, longitud, indexación, `get`, slicing,
selección por máscara booleana, conversión a `List<Measurement<Float>>`, retorno de un elemento
desde una función y visualización de elementos distintos. Las pruebas comparan intérprete y
nativo, verifican lowering por IR sin fallback, emiten C para WASI y terminan con
`live_allocations=0`. El perfil no habilita aritmética, broadcasting, reducciones ni covarianza.
`examples/measurement_covariance.ostrin` cubre la matriz densa acotada para mediciones escalares
adimensionales, incluyendo `J·Σ·Jᵀ`, diagonal consistente con sigma, matriz singular y rechazos
de entradas inválidas. La propagación factoriza la matriz semidefinida y suma proyecciones al
cuadrado; esto conserva, por ejemplo, una varianza de `0.01` cuando contribuciones grandes se
cancelan en una covarianza de rango uno no uniforme. Si la matriz validada no admite una
factorización numéricamente estable, la API devuelve un error explícito. Su prueba compara los
valores con una oracle analítica, corre en intérprete y
nativo, confirma lowering tipado, emite C para WASI y termina con `live_allocations=0`; no valida
covarianza dimensional ni sobre arrays.
`examples/measurement_quantity_native_smoke.ostrin` cubre construcción, suma/resta correlacionada,
productos entre dimensiones y proyección de incertidumbre estándar con paridad intérprete/native,
emisión WASI, `live_allocations=0`, cuerpo `main` generado por IR y cero divergencias de tipos.
`examples/measurement_lab.ostrin` valida además productos, el cociente de igual dimensión por
`div_quantity_same` y el cociente dimensional distinto; también genera `main` por IR y declara cero
divergencias. `examples/measurement_quantity_direct_shape.ostrin` cubre la superficie directa, sus
validaciones de entrada y el error explícito de `div_quantity` para dimensiones iguales; su `main`
también pasa por IR, coincide con la salida del intérprete, emite C para WASI y termina con
`live_allocations=0`. La frontera restante aquí es de API: el cociente adimensional requiere
`div_quantity_same`; no se reclama soporte general para otros programas o patrones de ownership.
`examples/viz_bars.ostrin` añade una serie de
cinco grupos y proyecta `values`/`uncertainties` hacia `std.viz.errorbars`, también con paridad
intérprete/native y una figura SVG reproducible en la galería. El Scientific Lab ejecuta el
ejemplo escalar con `ostrinc.wasm`. La prueba `std_measurements_scalar_matches_interpreter_and_native`
verifica además `live_allocations=0` en nativo y que el C emitido para WASI incluya la destrucción
recursiva de `Measurement<Float>`, `Sensitivity`, las etiquetas y las listas de fuentes. ASan/UBSan
ejecutaron esta prueba mediante `sanitizers.yml` en CI y terminaron correctamente en el run
`38173152897`. Esta evidencia no habilita todavía operaciones numéricas sobre `Array<Measurement<T>>`, integración nativa con `Quantity`,
intervalos de cobertura ni Monte Carlo.

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

`TemperaturePoint` es una excepción de rol, no una `Quantity<Temperature>` lineal. Si se habilita
una medición de punto absoluto, su valor nominal pertenecerá al espacio afín y la incertidumbre
pertenecerá al intervalo lineal `Quantity<Temperature>`. En una conversión Celsius/Fahrenheit, el
valor nominal aplica escala y offset; la incertidumbre aplica sólo el factor de escala. El modelo
actual `Sensitivity { coefficient: Float, sigma: Float }` no guarda unidad ni dimensión de fuente,
por lo que no puede expresar este contrato. El backend nativo rechaza actualmente
`Measurement<TemperaturePoint>`; el almacenamiento genérico en el intérprete no habilita
propagación, conversión ni salida científica para ese tipo.

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

### 5.5 Contrato propuesto para identidad de fuentes y composición

Esta sección fija el contrato de identidad de fuentes. La implementación escalar ofrece
`from_source(value, sigma, source_id, label)`: `source_id` es una clave explícita suministrada por
el usuario y `label` es sólo texto visible. Reutilizar el mismo ID correlaciona las contribuciones;
usar IDs distintos las mantiene independientes aunque compartan etiqueta. Un ID reutilizado con
otra `sigma` produce un error. `standard(value, sigma, label)` se conserva por compatibilidad y usa
la etiqueta también como ID, por lo que mantiene la semántica provisional antigua. La prueba
`std_measurements_scalar_matches_interpreter_and_native` cubre ambas reglas. El motor trata el ID
como clave opaca; su unicidad y reutilización correcta dentro del grafo quedan a cargo del usuario.
Por compatibilidad, un `source_id` explícito que coincida con la etiqueta de una fuente creada con
`standard` la correlaciona con esa fuente; el código que migra a `from_source` debe reservar IDs que
no colisionen accidentalmente con etiquetas históricas.

La representación futura debe separar tres conceptos:

| Concepto | Contrato propuesto |
| --- | --- |
| Identidad de fuente | `source_id` explícito, estable dentro del grafo y preservado al derivar una medición. En esta fase es una cadena suministrada por el usuario; no depende de una dirección de memoria, un timestamp ni del orden accidental de un recorrido. |
| Etiqueta | Texto opcional para mostrar el instrumento o el dato. Dos fuentes pueden compartir etiqueta sin quedar correlacionadas. |
| Nodo derivado | Expresión que conserva las referencias de fuente y sus sensibilidades; no duplica la identidad al hacer una copia del valor. |

`from_source` crea o reutiliza la fuente según el `source_id` que entregue el usuario; repetirlo es
una declaración explícita de correlación y requiere conservar la misma `sigma`. Una etiqueta
repetida no crea alias con este constructor. `standard(value, sigma, label)` conserva la regla
escalar vigente como conveniencia experimental; `covariance` ofrece un perfil denso acotado para
mediciones escalares adimensionales, mientras los arrays se limitan al perfil estructural y no
aceptan matrices de covarianza. La clave
estable suministrada por el usuario no se interpreta como identidad global entre ejecuciones.

Para `Array<Measurement<T>>` se adopta el siguiente contrato de composición:

- Cada elemento conserva su grafo de fuentes; la posición del elemento no es una fuente implícita.
- `slice`, `map`, concatenación y vistas preservan las referencias de las mediciones que reciben.
- Crear observaciones independientes dentro de un array exige una política explícita de fuentes;
  copiar el array no crea observaciones nuevas.
- `values` y `uncertainties` siguen siendo proyecciones ordenadas. Una reducción vuelve a construir
  el grafo y suma sensibilidades por identidad, de modo que una fuente compartida no se trate como
  independiente.
- La matriz de covarianza se expresa sobre un orden canónico de identidades de fuente, separado del
  orden de los elementos del array. Esto permite usar una matriz densa para un bloque pequeño y una
  representación dispersa para un conjunto grande sin cambiar el significado.

El perfil implementado `SourceCovariance` valida entre 1 y 64 IDs no vacíos y únicos, forma
`n × n`, entradas finitas, simetría y semidefinitud positiva. La tolerancia relativa está acotada a
`(0, 1e-6]` y su valor predeterminado es `1e-12`; los pares simétricos que difieren dentro de esa
tolerancia se promedian explícitamente antes de validar la semidefinitud y propagar. Se documenta
este ajuste acotado para que el resultado no dependa de cuál mitad de la matriz se lea. Una vista
normalizada por la entrada máxima finita se usa solo para comparar simetría y comprobar la
semidefinitud con el eigensolver; así, las comparaciones usan la tolerancia relativa sin sumar
magnitudes que puedan desbordarse. La propagación promedia las entradas originales antes de
factorizar, de modo que una entrada grande sin usar no borra correlaciones pequeñas de las fuentes
usadas. El punto medio evita perder covarianzas subnormales cuando ambas entradas tienen el mismo
signo y evita desbordarse cuando tienen signos opuestos. Las diagonales de cada fuente usada deben
coincidir con `sigma²` dentro de la misma tolerancia; si un `sigma` no nulo se eleva al cuadrado y
subdesborda a cero en `Float`, la propagación se rechaza explícitamente en vez de tratar la fuente
como exacta. Las fuentes declaradas pero no usadas tienen gradiente cero. La matriz se vuelve a
validar al propagar, porque sus arrays pueden cambiar después de la construcción. La fórmula es
`J · Σ · Jᵀ`, con `Jₛ = contributionₛ / sigmaₛ`. Para evaluar la forma cuadrática se calcula una
factorización semidefinida `Σ = L · Lᵀ` y se suman los cuadrados de las proyecciones `Lᵀ · J`;
cada proyección usa suma compensada. Así, las contribuciones grandes se cancelan antes de elevar el
residuo al cuadrado, incluso para la regresión no uniforme de rango uno con resultado `0.01`. Si la
factorización encuentra cancelación numéricamente irresuelta o una proyección subdesbordada, se
devuelve un error explícito en vez de informar una incertidumbre exacta. Los gradientes se
normalizan usando una potencia de dos exacta para conservar diferencias representables entre
contribuciones grandes. Los pivotes positivos numéricamente irresueltos se rechazan en vez de
introducir una componente espuria en matrices singulares. Los intermedios no finitos también se
rechazan. `ostrinc --compile` desactiva la contracción de multiplicación-suma del
compilador C (`-ffp-contract=off`): el backend WASI optimizado había fusionado operaciones y borrado
residuos que la suma compensada necesita preservar. La matriz Node WASI incluye estos casos de
cancelación. La API solo cubre `Measurement<Float>` con fuentes adimensionales; no acepta
`Measurement<Quantity<D>>`, `Array<Measurement<T>>`, covarianza entre elementos ni reducciones.
`examples/measurement_covariance.ostrin` comprueba sumas, diferencias, escalado, fuentes
independientes, una matriz singular, entradas de escalas muy distintas, cancelación de rango uno,
tolerancia de simetría y entradas inválidas con una oracle
analítica; también verifica paridad intérprete/nativo, C para WASI, lowering sin fallback y
`live_allocations=0`. El programa se añade a la matriz de 32 smoke tests WASI para ejecutarlo además
bajo Node WASI Preview 1.

La composición con `Quantity<D>` queda definida por tipos antes de elegir una representación de
runtime:

- `Measurement<Quantity<D>>` tiene incertidumbre de dimensión `D`; una incertidumbre de otra
  dimensión es un error de tipos.
- Una conversión de unidad transforma el valor nominal y su incertidumbre con la misma conversión,
  incluyendo la regla específica de unidades con offset.
- Suma y resta requieren dimensiones compatibles. Producto, cociente y potencias transforman la
  dimensión resultante y la dimensión de cada sensibilidad según la derivada de la operación.
- La unidad concreta de un `Quantity` no se usa como identidad de fuente. El instrumento o dataset
  debe declararse por separado.
- `Measurement<Quantity<D>>` no se convierte implícitamente a `Quantity<D>`; una proyección debe
  indicar que descarta la incertidumbre.

Los constructores `exact_quantity`, `unknown_quantity` y `from_quantity_source` crean
`Measurement<Quantity<D>>` en el intérprete. Los dos primeros permiten expresar los estados exacto
y de incertidumbre desconocida sin construir records manualmente; los tres rechazan nominales no
finitos tanto en el valor almacenado como después de convertirlo a la unidad coherente. `unknown_quantity`
no relaja la validez del nominal: sólo la incertidumbre queda desconocida.
`from_quantity_source` además:
conserva la contribución como `Quantity<D>`, guarda la sigma de la fuente en la unidad coherente
mediante `sigma.value() * sigma.unit_scale()` y registra la dimensión estática con
`value.dimension_key()`. Tanto el constructor como suma, resta, producto y `quantity_uncertainty` rechazan
nominales cuyo valor almacenado sea finito pero cuyo valor convertido a la unidad coherente no lo
sea; la validación de `quantity_uncertainty` también protege frente a records construidos
directamente por el usuario. `dimensional_measurements_reject_non_finite_coherent_values` cubre
constructor, suma/resta/producto, resultados con incertidumbre desconocida y records manuales.
La evidencia cubre `20 cm` y `0.2 m` como sigmas coherentes equivalentes,
además de rechazos para valores y sigmas no finitos, sigma negativa e identificador vacío. La
propagación de suma y resta también está disponible en el intérprete: `12 m ± 20 cm` y
`1200 cm ± 0.2 m` con la misma identidad producen una contribución de `40 cm` al sumar y `0 cm`
al restar. Reutilizar una identidad con otra sigma coherente produce un error; identificadores
distintos permanecen como fuentes independientes aunque compartan etiqueta y sigma. Cada
combinación valida que el resultado siga siendo finito. `mul_quantity` aplica la regla de producto
para las sensibilidades, combina fuentes compartidas y conserva `source_dimension` de cada origen
aunque la contribución pase a la dimensión del producto. Para dos lecturas `4 m ± 0.1 m` de la misma
regla, el producto tiene dimensión `Length^2` y contribución `0.8 m²`. `div_quantity_same` cubre
cocientes de cantidades de igual dimensión y convierte nominales y derivadas a unidades coherentes:
dos lecturas iguales de la misma fuente se cancelan a `1 ± 0`, mientras fuentes independientes con
`0.1 m` dan incertidumbre `0.03535533905932738`. El caso `3 m ± 0.1 m` por `2 s ± 0.2 s` verifica
contribuciones de origen `Length` y `Time` al resultado `Length*Time`: `0.2` y `0.6000000000000001`,
con incertidumbre combinada `0.632455532033676`. `quantity_uncertainty` suma los cuadrados de las contribuciones una vez convertidas a la unidad coherente y devuelve
`CoherentUncertainty<D>`; el valor numérico lleva la dimensión `D` en el tipo y el registro expone
`dimension_key`. Una medición exacta devuelve cero; incertidumbre desconocida y overflow producen
`Err`. La suma de dos fuentes independientes de `0.2 m` da `0.28284271247461906`, mientras que
la misma fuente compartida al sumarse da `0.4`.
`div_quantity` propaga ahora cocientes entre dimensiones distintas y conserva el tipo
`Measurement<Quantity<A/B>>`; las derivadas de numerador y denominador permanecen en la dimensión
del resultado y la sigma de cada fuente en su dimensión original. El ejemplo `10 m ± 0.1 m` sobre
`2 s ± 0.2 s` produce `5 m/s`, contribuciones coherentes `0.05 m/s` y `-0.5 m/s`, e incertidumbre
combinada `0.5024937810560445 m/s`. `div_quantity_same` conserva la API compatible que devuelve
`Measurement<Float>` para cocientes de igual dimensión. `div_quantity` devuelve un error explícito
si el cociente cancela las dimensiones; en ese caso se debe usar `div_quantity_same`, que representa
el resultado adimensional como `Measurement<Float>`. El backend nativo mantiene el rechazo explícito
hasta cerrar ownership, dimensión de incertidumbre y ABI.

Este prototipo habilita un perfil estructural unidimensional acotado para `Array<Measurement<Float>>`,
incluidos acceso por `get`, selección booleana, escape de elementos y visualización, pero no
operaciones numéricas sobre esos arrays ni covarianza de arrays. La evidencia de esta fase no generaliza el
perfil a otros payloads `Measurement<T>`, que requieren sus propias pruebas de paridad y ownership. El soporte de
`Measurement<Quantity<D>>` sigue siendo experimental y está limitado por la superficie realmente
verificada. El fixture `examples/measurement_quantity_direct_shape.ostrin` ya cubre su forma directa:
la prueba exige paridad de salida entre intérprete y binario nativo, lowering por IR (`ir=1`,
`hir=0`, `ast=0`), cero divergencias de tipo, emisión C para WASI y `live_allocations=0` en la
ejecución nativa con leak-check. Esta evidencia corresponde a ese fixture y sus operaciones; no
establece soporte general para toda combinación de dimensiones o contenedores. La propagación debe
mantener la dimensión del resultado en `contribution` y la sigma de fuente en su dimensión original.
Para ampliar el alcance se requieren pruebas adicionales de alias explícito, independencia de
etiquetas repetidas, reducciones con fuentes compartidas, covarianza dimensional y de arrays,
conversiones de unidades y ejecución WASI. También se necesitan leak-check, ASan y UBSan para los
buffers o grafos gestionados.

## 6. Operaciones y reglas

### 6.1 Operaciones algebraicas

La primera implementación debe cubrir suma, resta, producto, cociente, potencias enteras y las
conversiones de unidades ya soportadas. Para funciones diferenciables, el camino linealizado
usa las derivadas disponibles y conserva las dependencias de fuente.

El contrato escalar sin matriz usa la ley de propagación de incertidumbre de primer orden: si una
salida `y = f(x)` depende de fuentes con sensibilidades `cᵢ` y desviaciones estándar `σᵢ`, su
varianza es `Σᵢ (cᵢ σᵢ)²`. Para el perfil explícito `SourceCovariance`, la varianza general es
`J · Σ · Jᵀ`, donde `Jᵢ = contributionᵢ / σᵢ`; el ejemplo de covarianza fija y prueba ese contrato.
Sin matriz, `from_source` usa el `source_id` explícito para identificar una variable:
las contribuciones con el mismo ID se combinan antes de elevar al cuadrado, mientras que IDs
distintos se tratan como fuentes independientes. `standard` sigue usando la etiqueta como identidad
por compatibilidad experimental.

| Operación | Valor nominal | Sensibilidad de cada fuente `s` | Supuesto y límite |
| --- | --- | --- | --- |
| `exact(x)` | `x` | Ninguna; varianza cero | `x` debe ser finito. |
| `standard(x, σ, label)` | `x` | `c_label = 1` | Compatibilidad experimental: usa `label` como ID; `x` y `σ` finitos, `σ ≥ 0`, mismo ID con una sola `σ`. |
| `from_source(x, σ, id, label)` | `x` | `c_id = 1` | `id` no vacío; la etiqueta es sólo descriptiva; el mismo ID requiere una única `σ`. |
| `scale(a, k)` | `k·a` | `k·cₐ,s` | Lineal y exacta respecto a `k`; el valor desconocido sigue siendo desconocido. |
| `add(a,b)` / `sub(a,b)` | `a ± b` | `cₐ,s ± c_b,s` | Fuentes de igual ID son correlacionadas y se combinan con signo antes de calcular varianza. |
| `mul(a,b)` | `a·b` | `b·cₐ,s + a·c_b,s` | Aproximación de Taylor de primer orden alrededor de los valores nominales; se omiten términos de orden superior. |
| `div(a,b)` | `a/b` | `cₐ,s/b − a·c_b,s/b²` | Igual aproximación local; `b ≠ 0`; no cubre denominadores cercanos a cero ni distribuciones fuertemente no lineales. |

Las pruebas analíticas actuales comprueban que para `x=12.30±0.02`, `x*x` produce incertidumbre
`2·12.30·0.02 = 0.492` (la misma fuente aparece en ambos operandos), y que para una fuente
independiente `y=1.50±0.03`, `u(x*y) = √((y·0.02)² + (x·0.03)²)`. La validación se compara con
estas derivadas y la ley de propagación documentada por [NIST TN 1297, Appendix A](https://www.nist.gov/pml/nist-technical-note-1297/nist-tn-1297-appendix-law-propagation-uncertainty); no sustituye una validación de segundo orden ni Monte Carlo.

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

`Array<Measurement<Float>>` ya tiene un perfil estructural experimental después de fijar el contrato
de identidad de §5.5. La API de series sigue ofreciendo una proyección explícita para
`List<Measurement<Float>>`; la siguiente fase debe ampliar el perfil a otros payloads y añadir:

- valores nominales;
- incertidumbres por elemento;
- matriz de covarianza indexada por fuentes, cuando el modelo la requiera;
- reducciones que respeten correlaciones;
- vistas sin copiar metadatos innecesariamente;
- una política explícita para crear fuentes nuevas por elemento.

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
2. Implementar `Exact` y `Standard` para escalares en el intérprete con operaciones básicas. **En
   curso:** `std.measurements` experimental cubre también `Unknown`.
3. Añadir fuentes correlacionadas y pruebas de cancelación. **Disponible para escalares:**
   `from_source` combina por ID explícito y `standard` conserva la semántica histórica por etiqueta;
   intérprete/nativo/WASM tienen pruebas de paridad.
4. **Implementación experimental para el alcance verificado:** integrar `Measurement<Quantity<Dim>>`
   y conversiones de unidades; `measurement_quantity_direct_shape.ostrin` fija el comportamiento
   concreto cubierto por intérprete y nativo.
5. **Implementado para ese fixture:** lowering a IR tipado, emisión C para WASI y comprobación
   nativa de paridad y ownership con leak-check. La generalización a todas las estructuras genéricas
   y contenedores sigue abierta.
6. **Parcial:** proyectar series escalares a valores e incertidumbres para error bars y ofrecer
   `SourceCovariance` experimental hasta 64 fuentes escalares adimensionales; siguen pendientes
   las matrices para arrays/dimensiones y la salida de contribuciones.
7. Añadir propagación Monte Carlo explícita y semilla reproducible.
8. **Parcial:** las series escalares proyectan incertidumbres estándar a `std.viz.errorbars`;
   falta conectar `Quantity`, covarianza dimensional/de arrays, bandas con nivel declarado y
   procedencia completa.
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

