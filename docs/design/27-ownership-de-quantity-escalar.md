# 27. Ownership de `Quantity` escalar y etiquetas de unidad

*Estado: diseño técnico para revisión. No habilita todavía la bajada de
`Figure.unit_line`/`unit_scatter` a IR/C.*

Depende de [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md),
[11-modelo-de-memoria.md](11-modelo-de-memoria.md),
[18-modelo-de-memoria-nativo.md](18-modelo-de-memoria-nativo.md),
[19-jerarquia-numerica-y-arrays.md](19-jerarquia-numerica-y-arrays.md),
[20-hir-y-ir.md](20-hir-y-ir.md) y
[21-distribucion-y-wasm.md](21-distribucion-y-wasm.md).

Este documento resuelve una pregunta concreta que quedó abierta al intentar
llevar las instanciaciones genéricas de visualización con cantidades al emisor
IR/C: **¿quién mantiene viva la etiqueta de unidad de un `Quantity` que se
extrae de un array?** El objetivo es cerrar el contrato antes de cambiar el
ratchet del compilador.

## 1. Evidencia actual

El tipo estático `Quantity<D>` es un valor. En el runtime C se representa hoy
como:

```c
typedef struct { double v; const char* u; } Qty;
```

`u` puede apuntar a:

- un literal estático (`"m"`, `"s"`, `"km/h"` generado por el compilador);
- una cadena administrada creada por `ostrin_unit_cat` o
  `ostrin_unit_combine`;
- la etiqueta propiedad de un `Array<Quantity<D>>` (`array->unit`), que es un
  préstamo mientras el array esté vivo.

El registro del runtime conoce la referencia de una cadena dinámica porque fue
creada con `ostrin_alloc`. `ostrin_retain` y `ostrin_release` son no-op para
literales estáticos y cuentan las cadenas dinámicas. Los arrays ya tienen un
contrato explícito: `ostrin_qa_tag` retiene una etiqueta prestada,
`ostrin_qa_tag_owned` consume una referencia recién creada y el destructor del
array libera su etiqueta.

El contrato equivalente no existe para el `Qty` escalar. El emisor actual
puede generar:

```c
Qty result = (Qty){ Array_Float_max(array), array->unit };
```

La copia no retiene `array->unit`. Si el análisis de último uso libera `array`
antes de usar `result`, el destructor del array libera la cadena y `result.u`
queda colgando.

El experimento conservado como artefacto local fuera del repositorio durante
esta auditoría hizo visible la secuencia con `examples/viz_units.ostrin`: después de
`distance.max()` se liberó el array y el uso posterior de la unidad produjo
`runtime error: unknown unit` con bytes ya liberados. El parche experimental no
se integró. El estado verificado de `main` mantiene esas instanciaciones en
fallback HIR/AST y conserva la paridad existente.

## 2. Invariantes que debe preservar la solución

1. `Quantity<D>` sigue siendo un valor desde el lenguaje. El usuario no
   escribe referencias, lifetimes ni llamadas a `retain`/`release`.
2. La dimensión `D` continúa siendo estática y la unidad textual continúa
   siendo un dato de ejecución.
3. Una etiqueta de unidad es inmutable. Copiar un `Quantity` copia el valor
   numérico y crea una referencia válida a la misma etiqueta; no copia ni
   modifica el texto.
4. Un `Qty` que sale de una expresión nativa devuelve una referencia propia
   para su etiqueta, salvo que el resultado sea transferido como movimiento.
5. Un parámetro `Qty` es prestado durante la llamada. El callee no lo libera.
   Si lo devuelve o lo almacena, debe adquirir una referencia propia.
6. Un `Qty` extraído de un array, lista, record, `Option` o `Result` adquiere
   su propia referencia antes de que el contenedor pueda ser liberado.
7. Una vista de array conserva el contrato actual del array: la vista retiene
   el buffer y la etiqueta compartida; un `Qty` escalar no conserva el buffer,
   conserva solamente la etiqueta.
8. Ninguna ruta que no pueda demostrar estas reglas puede entrar en IR/C. Debe
   continuar en el fallback verificado hasta que su contrato sea implementado.

## 3. Representación elegida para la primera implementación

Se conserva la representación binaria interna `Qty { double v; const char* u; }`
durante esta migración. No se añade todavía un objeto `UnitLabel` ni se cambia
la forma del array. Esto mantiene el ABI interno de C, el runtime WASI y los
literales estáticos pequeños.

La propiedad se vuelve explícita en el ABI del runtime y en la IR, no en un bit
oculto dentro del struct. El estado de cada `ValueId` será uno de:

| Estado IR | Significado | Acción al copiar | Acción al destruir |
|---|---|---|---|
| `BorrowedQty` | El `Qty` pertenece al caller o a un contenedor vivo | `ostrin_qty_copy` si escapa | ninguna en el callee |
| `OwnedQty` | El `ValueId` posee una referencia a `u` | movimiento o `ostrin_qty_copy` según el alias | `ostrin_qty_release` en último uso |
| `StaticQty` | `u` es literal o símbolo estático | copia simple es segura | no-op |

`StaticQty` es una optimización que el emisor puede inferir, no una nueva
categoría visible en el lenguaje. Si el análisis no puede probar que la
etiqueta es estática, debe tratarla como `OwnedQty`/`BorrowedQty`.

### 3.1 ABI tipado

`qty_runtime.c` añadirá helpers pequeños, todos internos al C generado:

```c
static void ostrin_qty_retain(Qty q);       // retiene q.u si es dinámico
static void ostrin_qty_release(Qty q);      // libera q.u si es dinámico
static Qty ostrin_qty_copy(Qty q);          // copia numérica + retain de q.u
static Qty ostrin_qty_from_borrowed(double v, const char* u);
static Qty ostrin_qty_from_owned(double v, const char* u);
```

Sus contratos serán:

- `ostrin_qty_retain` y `ostrin_qty_release` delegan en el registro existente;
  no liberan literales ni `NULL`.
- `ostrin_qty_copy` devuelve un `OwnedQty`; nunca devuelve una referencia a un
  buffer del array sin retenerla.
- `ostrin_qty_from_borrowed` retiene `u` y devuelve un `OwnedQty`.
- `ostrin_qty_from_owned` consume la referencia inicial de `u` y devuelve un
  `OwnedQty`; se usa después de `ostrin_unit_combine`.
- Las funciones que aceptan `Qty` (`add`, `sub`, comparaciones, métodos) toman
  un préstamo durante la llamada. Las funciones que devuelven `Qty` devuelven
  una referencia propia.

El código generado no debe usar `ostrin_retain((void*)q.u)` como sustituto
disperso de estos helpers. El helper tipado es necesario para que los literales,
`NULL`, resultados dinámicos y futuras representaciones de unidad compartan
una sola regla.

### 3.2 Productores y consumidores

| Operación | Contrato de etiqueta |
|---|---|
| Literal `5 m`, `as km` | `from_borrowed` con símbolo estático |
| `a + b`, `a - b`, escalares | devuelve copia propia de la etiqueta elegida |
| `a * b`, `a / b` | `unit_combine` + `from_owned`; si la dimensión se cancela, libera la etiqueta temporal después de convertir el valor |
| `array.max/min/mean/percentile/get` | `from_borrowed(result, array->unit)` |
| `array[index]` | mismo contrato que una reducción |
| `array.to_list()` | el helper de lista llama `qty_retain` por elemento |
| `List<Quantity>.get/remove/find` | devuelve una copia propia antes de que el contenedor pueda morir |
| `Array<Quantity>` slice/transpose/cumsum | el array retiene su propia etiqueta; el resultado escalar sigue usando `from_borrowed` |
| `Quantity.to_string`, `value`, comparaciones | solo préstamo; no escapan `u` |
| campo de record / `Option` / `Result` | el proyector o constructor clona el `Qty` cuando crea un alias |

El destructor de un contenedor que almacena `Quantity` usa `qty_release`, no un
cast genérico sobre la dirección del struct. Nunca se libera `&q` como si el
`Qty` completo fuera una allocation.

## 4. Integración con HIR, IR y aliasing

### 4.1 HIR y backend C heredado

El backend HIR/AST sigue siendo necesario durante la migración. Debe consumir
los mismos helpers para que una función que aún esté en fallback no tenga una
semántica de ownership distinta de una función IR:

- una expresión que produzca `Qty` recibe la referencia propia del helper;
- un binding local que copie una expresión usa `qty_copy` o una transferencia
  probada;
- los parámetros no se liberan dentro del callee;
- la salida de una función transfiere la referencia propia al caller;
- los campos de records, listas y wrappers retienen al almacenar y liberan al
  destruir.

Hasta que estas reglas estén implementadas en ambas rutas, el dispatcher debe
rechazar la ruta IR para una función que mezcle `Quantity` escalar con un
contenedor que pueda liberar la etiqueta.

### 4.2 IR y último uso

Cuando el ABI esté listo, `Ty::Quantity(_)` puede entrar en
`ownership::requires_management`. La modificación debe ser simultánea en el
análisis y el emisor:

1. `Param` se marca como prestado.
2. `Call`, `MethodCall`, `Binary`, `Unary` y productores `Opaque` de `Qty`
     generan valores propios.
3. `Move` transfiere la referencia y elimina la responsabilidad del origen
   cuando no hay usos posteriores.
4. `Field`, `Index`, `PatternBind`, `TryValue` y `TryErrorValue` son aliases:
   el destino recibe un `qty_retain` antes de que el origen pueda morir.
5. `Phi` retiene por arista cuando dos caminos comparten el mismo valor y
   transfiere la referencia cuando solo hay un consumidor.
6. `Return` transfiere un `OwnedQty` al caller; un parámetro prestado que se
   devuelve debe clonarse antes de la terminación.
7. `Retain`/`Release` de la IR se emiten como `ostrin_qty_retain/release`, no
   como un cast de `Qty` a `void*`.

El análisis debe tratar `Opaque` que extrae un escalar desde un array como un
productor con ownership conocido. Si encuentra una operación C opaca cuyo
contrato no declara si devuelve préstamo, copia o transferencia, debe marcar
la función como no elegible para IR/C.

### 4.3 Records y wrappers

El soporte de `Quantity` en `retain_payload`/`release_payload` debe ser
recursivo. La primera fase puede habilitar:

- `record` no recursivo con campos `Quantity` y campos escalares;
- `List<Quantity<D>>`;
- `Option<Quantity<D>>` y `Result<Quantity<D>, E>` con payloads soportados;
- aliases locales y `Phi` lineales.

Records recursivos, closures que escapan y wrappers anidados deben permanecer en
fallback hasta tener destructores y pruebas específicas. No se debe aumentar
la cobertura del ratchet solo porque el C compile.

## 5. Arrays y visualización

El array continúa siendo propietario de su buffer y de una referencia a su
etiqueta. Una reducción escalar no conserva el array: conserva solo una
referencia a la etiqueta mediante `from_borrowed`. Por ello esta secuencia debe
ser válida después de que el array muera:

```ostrin
values = array([1 m, 2 m]) * array([1 m, 2 m])
peak = values.max()
print(peak)
```

`peak` debe imprimir `4 m^2` aunque el análisis libere `values` inmediatamente
después de `max`. La misma regla cubre `values[0]`, `percentile`, `mean` y
`to_list`.

Con este contrato, el experimento de `viz_units` podrá habilitarse de forma
incremental: primero se valida `distance.max()` y la conversión posterior;
después se habilitan las instanciaciones genéricas de `unit_line` y
`unit_scatter`; finalmente se retira el fallback solo cuando el informe nativo,
WASI, sanitizers y la prueba diferencial sean verdes.

## 6. Native, WASI y browser

### Native

- El runtime reutiliza el registro actual y sus locks. No se inventa un
  segundo contador para etiquetas.
- Las etiquetas son inmutables y pueden compartirse entre tareas; la primera
  implementación conserva el locking del registro incluso en programas con
  `--native-threads`.
- El C generado por HIR e IR comparte `qty_runtime.c` y los helpers tipados.
- `--leak-check` debe terminar en `live_allocations=0` tanto para etiquetas
  estáticas como para unidades dinámicas (`m^2`, `m/s`, etc.).

### WASI

- `wasm32-wasip1` usa la misma representación y helpers; no depende de
  pthreads ni de un allocator externo.
- Debe comprobarse que los punteros de 32 bits no se truncan al pasar por
  `ostrin_qty_retain/release`.
- La salida del programa y el número de allocations vivas deben coincidir con
  native e intérprete.

### Browser

El playground actual ejecuta el intérprete y no genera C. No se cambia su
semántica. Si en el futuro se ofrece un backend C/WASM para programas de
usuario, el contrato de etiquetas debe ser el mismo y no se permite exponer
`Qty.u` a JavaScript como un puntero.

## 7. Matriz de pruebas de aceptación

La implementación no se considera terminada hasta que cada fila tenga prueba
positiva, salida diferencial y leak-check cuando aplique:

| Caso | Native | WASI | Sanitizers | Criterio |
|---|---:|---:|---:|---|
| `array([1 m, 2 m]).max()` después del último uso del array | sí | sí | ASan/UBSan | sin UAF, salida estable |
| producto dinámico `m * m` y conversión posterior | sí | sí | ASan/UBSan | `m^2` correcto, cero fugas |
| `index`, `mean`, `percentile` y `to_list` | sí | sí | ASan/UBSan | cada copia conserva su etiqueta |
| parámetro `fn id(q) -> Quantity` y retorno del parámetro | sí | sí | ASan/UBSan | el caller recibe una copia viva |
| alias por `Phi`, `if`, loop y `match` | sí | sí | ASan/UBSan | una liberación por referencia |
| record con campo `Quantity` | sí | sí | ASan/UBSan | destructor recursivo correcto |
| `Option`/`Result` de `Quantity` | sí | sí | ASan/UBSan | solo se libera el payload activo |
| `List<Quantity>` y `Array<Quantity>` | sí | sí | ASan/UBSan | no hay doble liberación |
| `examples/viz_units.ostrin` | sí | sí | ASan/UBSan | desaparece el fallback solo con evidencia |
| literales estáticos (`m`, `s`, `km/h`) | sí | sí | ASan/UBSan | `retain/release` no intenta liberar literales |

Además, la suite debe verificar:

```text
cargo fmt --check
cargo check
cargo test --manifest-path compiler/Cargo.toml
ostrinc --native-type-report examples/viz_units.ostrin
ostrinc --emit-c --target wasm32-wasi examples/viz_units.ostrin
```

El informe de `viz_units` debe seguir indicando fallback mientras el diseño no
esté implementado. La regresión que fija ese límite se puede retirar únicamente
en el mismo PR que añada las pruebas de la fila completa y reduzca el contador
de fallback con paridad.

## 8. Orden de implementación

1. Añadir los helpers tipados a `qty_runtime.c` y pruebas unitarias del runtime
   para literal, etiqueta dinámica, copia, movimiento y liberación.
2. Cambiar productores de `Qty` en HIR/C e IR/C para declarar si devuelven
   préstamo, referencia propia o transferencia. No habilitar aún el ratchet.
3. Extender `retain_payload`, `release_payload`, listas, records y wrappers;
   añadir `Ty::Quantity` a `requires_management` solo cuando el emisor de
   `Retain`/`Release` esté listo.
4. Activar primero el caso reducido de `max`/`index` y ejecutar native, WASI,
   ASan, UBSan e intérprete diferencial.
5. Habilitar las instanciaciones genéricas de visualización y convertir la
   regresión de fallback en una aserción `ir=1, hir=0, ast=0` para el entry
   point, manteniendo un baseline explícito para `std.viz`.
6. Medir el coste de las copias de etiquetas antes de introducir interning o
   una representación `UnitLabel` opaca. La optimización no debe preceder a la
   prueba de ownership.

## 9. Decisiones que no se toman todavía

- No se cambia `Qty` a un puntero de heap: aumentaría el coste de cada
  operación numérica y rompería más ABI de C/WASI del necesario.
- No se introduce un interner global de unidades: puede reducir allocations,
  pero debe resolver lifetime, concurrencia y límites del catálogo primero.
- No se considera suficiente retener solo en `Array_Float_max`: el mismo alias
  aparece en indexación, listas, records, `Phi`, retornos y wrappers.
- No se afirma que el problema esté resuelto mientras `Quantity` siga fuera de
  `requires_management` y las copias de `Qty` puedan emitirse como asignación
  C directa.
