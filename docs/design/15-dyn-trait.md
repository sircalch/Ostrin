# Ostrin — Diseño core: `dyn Trait` (polimorfismo dinámico)

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [03-traits.md](03-traits.md), [11-modelo-de-memoria.md](11-modelo-de-memoria.md), [13-map-set-y-literales-de-coleccion.md](13-map-set-y-literales-de-coleccion.md)

Decisión de fondo ya cerrada:
- **`dyn Trait` existe, como mecanismo de escape explícito** — no reemplaza a los genéricos (que siguen siendo la forma por defecto, resuelta en compilación), se añade aparte para el único caso real que los genéricos y los `enum` no resuelven bien: una colección de tipos concretos distintos, potencialmente definidos en módulos o librerías diferentes, que comparten un trait pero no un `enum` cerrado que los una.

---

## 1. Por qué hace falta esto además de genéricos y `enum`

- Un genérico `<T: Shape>` resuelve "esta función funciona para cualquier tipo que implemente `Shape`" — pero **una sola llamada** siempre trabaja con **un solo `T` concreto** a la vez (el compilador genera una versión especializada del código por cada `T` usado, "monomorphization"). No sirve para "una lista que mezcla círculos, rectángulos y triángulos al mismo tiempo".
- Un `enum` (documento 05) sí modela "una de varias formas concretas" en una sola lista — pero exige que **todas** las variantes se conozcan y se declaren en un solo lugar. No sirve cuando una librería externa quiere añadir su propio tipo que implementa `Shape` sin tocar el `enum` de nadie (el caso típico de un sistema de plugins, o simplemente una librería de formas geométricas que otros extienden).
- `dyn Trait` cubre exactamente ese hueco: valores de tipos concretos distintos y potencialmente desconocidos en el momento de compilar la colección, unidos solo por implementar el mismo trait.

## 2. Sintaxis y uso

```ostrin
trait Shape {
    fn area(self) -> Quantity<Length^2>
}

record Circle { radius: Quantity<Length> }
record Rectangle { width: Quantity<Length>, height: Quantity<Length> }

impl Shape for Circle {
    fn area(self) -> Quantity<Length^2> { 3.14159 * self.radius * self.radius }
}
impl Shape for Rectangle {
    fn area(self) -> Quantity<Length^2> { self.width * self.height }
}

shapes: List<dyn Shape> = [Circle { radius: 2 m }, Rectangle { width: 3 m, height: 4 m }]

for shape in shapes {
    print(shape.area())
}
```

- `dyn Trait` es un tipo en sí mismo: "cualquier valor, sea cual sea su tipo concreto, que implemente `Trait`". Se usa donde iría cualquier otro tipo — como parámetro de función, tipo de elemento de `List`/`Map`/`Set`, tipo de retorno, campo de un `record`.
- **Asignar un valor concreto a una posición de tipo `dyn Trait` es implícito**, sin conversión explícita (`Circle { ... }` se acepta directamente donde se espera `dyn Shape`) — esto no es una excepción a "sin coerción implícita" (documento 01, §3.3): esa regla habla de coerciones que pierden o inventan información (un escalar convirtiéndose en una cantidad física con unidad arbitraria); aquí no se pierde ni se inventa nada, solo se "olvida" temporalmente el tipo concreto para tratarlo de forma uniforme — es la misma clase de operación que asignar cualquier valor a una variable de un tipo menos específico en cualquier lenguaje con subtipado de interfaces.
- Dentro de una función genérica sobre `T: Shape`, cada llamada a `shape.area()` se resuelve **en compilación**, sin costo extra (se sabe exactamente qué código ejecutar). Sobre un `dyn Shape`, la llamada se resuelve **en tiempo de ejecución** (una tabla de métodos por tipo, consultada en cada llamada) — el precio a pagar por la flexibilidad de mezclar tipos que el compilador no puede enumerar de antemano.

## 3. Traits "compatibles con `dyn`" (object safety)

No todos los traits pueden usarse como `dyn Trait`. Un trait es válido para esto solo si **todos** sus métodos cumplen:

1. **Ningún método tiene sus propios parámetros genéricos.** Un método `fn transform<U>(self, f: fn(T) -> U) -> U` no se puede despachar dinámicamente, porque haría falta generar una versión distinta por cada `U` posible en tiempo de ejecución — lo opuesto a lo que la resolución dinámica puede hacer.
2. **Ningún método devuelve `Self` por valor.** A través de un `dyn Trait` no se conoce el tipo concreto, así que no hay forma de nombrar "un `Self` nuevo" como tipo de retorno — sí se puede devolver `dyn Trait` (el mismo trait, sin comprometerse al tipo concreto), pero no `Self`.

```ostrin
trait Cloneable {
    fn duplicate(self) -> Self   // devuelve Self: NO es compatible con dyn
}
```

```text
Error OSTRIN-E1130
Trait 'Cloneable' cannot be used as 'dyn Cloneable': method 'duplicate' returns 'Self'.
A dyn-compatible trait's methods cannot return the concrete implementing type,
since it is erased at the point of dynamic dispatch.
```

Intentar escribir `dyn Cloneable` en cualquier posición de tipo, para un trait que no cumple estas reglas, es error de compilación en el sitio donde se usa `dyn Cloneable`, señalando cuál de las dos reglas rompe.

## 4. `mut self` a través de `dyn Trait`

La distinción `self`/`mut self` (documento 03, §1.1) se conserva sin cambios: llamar un método `mut self` de un `dyn Trait` exige que el binding que lo sostiene sea `mut`, exactamente igual que con cualquier otro valor:

```ostrin
mut shapes: List<dyn Resizable> = [...]
shapes.get(0).unwrap().resize(2.0)   // OK si 'resize' es 'mut self' y 'shapes' es mut
```

## 5. Representación en memoria

Un valor `dyn Trait` no tiene tamaño conocido en compilación (podría ser un `Circle` pequeño o un `record` grande con muchos campos) — igual que ocurre con los campos recursivos de un `enum` (documento 11, §5), el compilador lo asigna automáticamente en el heap. Concretamente, un `dyn Trait` se representa con dos punteros: uno al valor real (gestionado por ARC como cualquier otro valor con identidad, documento 11, §2) y otro a una tabla de métodos (una función por cada método del trait, apuntando a la implementación concreta de ese tipo). Esto es, de nuevo, invisible para el programador — coherente con la decisión de no exponer `Box<T>` explícito (documento 11): el compilador ya necesitaba resolver indirección automática para casos como este.

## 6. Combinando varios traits

Igual que un trait bound de un genérico admite varios traits con `+` (documento 03, §3), un `dyn Trait` también:

```ostrin
shapes: List<dyn Shape + Printable> = [...]
```

Cada valor de la lista debe implementar **ambos** traits — el compilador genera una tabla de métodos combinada por detrás; sigue sin ser visible ni configurable desde el código Ostrin.

## 7. Frontera actual del backend y contrato para migrar a HIR/IR

La representación no debe confundirse con soporte nativo completo. El ABI C
usa una pareja tipada equivalente a:

```c
typedef struct {
    void *self;
    const Trait_VTable *vtable;
} Trait_Dyn;
```

Las vtables son inmutables y contienen slots de métodos y hooks de lifecycle
para el payload. HIR/C ya representa `Ty::Dyn` como el struct completo, hace
boxing concreto→dyn para records registrados, despacha llamadas por la vtable y
usa sus hooks al copiar, reemplazar, retornar, destruir y almacenar valores en
`List<dyn Trait>`. `dyn_trait.ostrin` (incluidos `main` y el `fold` indirecto)
ahora genera HIR/C; las pruebas cubren lista heterogénea, argumento/retorno,
iteración, alias, Phi y loop-Phi con `live_allocations=0` en los casos focales.
IR todavía no tiene nodos dedicados `DynBox`/`DynCall`; el único cuerpo dyn
restante del ledger es el boxing de record genérico con método default
(`native_trait_defaults.ostrin::main`).
Los campos de records no genéricos y el entorno HIR de closures ahora preservan
la pareja `data/vtable`: `native_dyn_trait_record_field_replacement_preserves_pair_ownership`
prueba reemplazo, alias y autoasignación; `native_dyn_trait_closure_capture_outlives_the_source_scope`
prueba que el payload sobrevive al scope que lo creó y se libera al destruir la
closure. Ambos tienen paridad intérprete/native y `live_allocations=0`. La matriz
WASI previa cubre la lista heterogénea; WASI y ASan/UBSan para los dos casos
nuevos se esperan del CI actual. Captura por tarea, cancelación, campos genéricos
y default dispatch en `Box<Int>` siguen fuera del ABI HIR/C verificado.

La migración queda condicionada a un contrato único para AST, HIR, IR, WASI y
el runtime:

1. El valor tipado debe conservar `data`, `vtable` y el estado de ownership en
   cada construcción, argumento, retorno, campo, colección, `Phi`, closure,
   tarea, cancelación y `drop`.
2. Cada vtable debe incluir operaciones de ciclo de vida (`retain` y
   `release`, o una ABI equivalente demostrada) además de sus slots de método;
   un receptor borrado no puede depender de una limpieza global al salir del
   proceso.
3. El lowering debe rechazar de forma determinista traits no object-safe,
   llamadas indirectas no representables y combinaciones genéricas cuyo
   ownership no esté probado. No se permite convertir esos casos en un cast
   opaco.
4. La aceptación mínima exige paridad intérprete/native, emisión y ejecución
   WASI, `live_allocations=0`, ASan y UBSan para boxing, lista heterogénea,
   argumento/retorno, método por defecto, `Phi`, closure, tarea y cancelación.

### 7.1 Estado de ownership y transiciones obligatorias

El ABI propuesto trata al trait object como un valor gestionado con dos partes
indivisibles. `data` apunta al payload y `vtable` describe tanto los métodos
como el destructor del payload; nunca se permite conservar uno sin el otro.
Cada operación debe respetar estas transiciones:

| Operación | Precondición | Acción requerida | Postcondición |
|---|---|---|---|
| `box` | payload concreto válido | asignar payload y seleccionar vtable | una referencia propietaria al par `data/vtable` |
| copia/argumento/campo | par no nulo | llamar `retain(data, vtable)` | dos referencias válidas al mismo par |
| retorno/colección | referencia válida | transferir o retener según el modo de paso | el receptor tiene exactamente una referencia nueva |
| `Phi`/reemplazo | ambas ramas pueden poseer | retener la rama elegida y liberar la descartada | una sola referencia al valor seleccionado |
| `drop`/cancelación | referencia válida | llamar `release(data, vtable)` una vez | el payload queda destruido cuando llega a cero |

El contador, si existe, pertenece al bloque de `data` y no a la vtable. La
vtable es inmutable, compartible y no se libera por cada instancia. Una vtable
mal formada, un `data == NULL` inesperado o una combinación de tipos que no
pueda conservar estas invariantes debe producir un diagnóstico de lowering;
no puede degradarse a un cast opaco ni a una liberación global al finalizar el
proceso.

La siguiente etapa debe propagar la representación única a IR y a campos,
colecciones y entornos de ejecución, con `drop` explícito donde corresponda.
La migración de cada fallback restante se acepta solo cuando su prueba nativa demuestra las cuatro
transiciones anteriores además de la matriz de paridad, WASI y sanitizers de la
sección 7.

Hasta que exista esa evidencia, los casos fuera de la cobertura HIR/C permanecen
en los fallbacks AST enumerados; la frontera es intencional y forma parte del
contrato de v0.2.

---

## 8. Preguntas abiertas para la siguiente sesión de diseño

1. **`match` sobre `dyn Trait`** — no tiene sentido igual que sobre un `enum` (no hay un conjunto cerrado de "variantes"); si hace falta recuperar el tipo concreto desde un `dyn Trait` (downcasting), sería un mecanismo aparte, no diseñado aquí.
2. Pendientes previos siguen abiertos: `as D`, `select` sobre canales, cancelación de tareas, elisión de ARC, operadores bit a bit, ordenación general.
