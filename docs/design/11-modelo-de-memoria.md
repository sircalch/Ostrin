# Ostrin — Diseño core: Modelo de Memoria

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md), [06-rangos-e-iteradores.md](06-rangos-e-iteradores.md), [09-concurrencia.md](09-concurrencia.md)

Decisiones de fondo ya cerradas:
- **ARC** (conteo de referencias automático), no recolector de basura de trazado.
- **Representación en memoria transparente**: el programador nunca elige explícitamente "esto va al heap" (no existe un `Box<T>`); el compilador decide, de forma correcta gracias a que los datos inmutables son indistinguibles copiados o compartidos.

Este documento completa el hueco más importante señalado en la revisión de consistencia (documento 10): cómo se libera la memoria de un valor cuando ya no se usa, y cómo esa respuesta encaja con lo que ya se decidió en variables (documento 01) y concurrencia (documento 09).

---

## 1. La regla central: identidad solo donde hay mutabilidad

Ya lo insinuaba el documento 09: **lo único que necesita seguimiento cuidadoso es lo que puede cambiar.** El modelo de memoria de Ostrin extiende esa misma idea de la seguridad de concurrencia a la gestión de memoria en general:

- Un valor **completamente inmutable** (`Int`, `Float`, `Quantity<D>`, un `record` con todos sus campos inmutables, una `List<T>` de elementos inmutables) se trata conceptualmente por **valor**: asignarlo o pasarlo a una función "copia" el valor. Como nada puede mutarlo, al programador nunca le importa si por debajo el compilador realmente copió los bytes o compartió una referencia a la misma zona de memoria — son indistinguibles desde el código Ostrin. El compilador es libre de elegir lo que sea más eficiente (copiar valores pequeños, compartir por referencia los grandes) sin que eso sea una decisión visible ni controlable desde el lenguaje (decisión de fondo ya tomada arriba).
- Un valor que contiene **al menos un campo `mut`** tiene **identidad**: dos bindings que se refieren a "el mismo" record mutable ven los cambios del otro (el ejemplo de `Fibonacci` en el documento 06 depende exactamente de esto — `self.current` tiene que seguir siendo el mismo storage entre llamadas a `.next()`). Esto solo es posible si, por debajo, ese valor vive en el heap y se accede por referencia. Aquí es donde entra ARC.

## 2. ARC — conteo de referencias automático

Todo valor con identidad (con algún campo `mut`, transitivamente) vive en el heap con un contador de referencias:

- Al crear el valor, el contador empieza en 1.
- Al copiar una referencia a ese valor (asignarlo a otro binding, capturarlo en un closure de la misma tarea, pasarlo a una función), el contador se incrementa.
- Al salir de scope un binding que lo referenciaba (o al ser reasignado a otra cosa), el contador se decrementa.
- Cuando el contador llega a **cero**, el valor se libera inmediatamente — no en un momento futuro indeterminado, sino en el punto exacto del programa donde deja de tener referencias. Esto es lo que permite liberación **determinista** de recursos (ver §4, `Drop`).

```ostrin
record Simulation {
    mut step: Int
    mut state: List<Float>
}

fn run() {
    sim = Simulation { step: 0, state: initial_state() }   // contador = 1
    process(sim)                                             // pasado por referencia, contador = 2 durante la llamada
    // al volver de process(), contador vuelve a 1
}   // 'sim' sale de scope aquí, contador llega a 0 → se libera inmediatamente
```

### 2.1 Por qué el conteo no necesita ser atómico en todos lados

Incrementar/decrementar un contador de referencias normalmente exige una operación atómica si el valor puede ser accedido desde más de una tarea a la vez (el costo real de ARC en lenguajes como Swift). Ostrin puede evitarlo en el caso más común gracias a la regla de movimiento ya definida en el documento 09:

- Un valor `mut` **movido** a través de un canal (`ch.send(valor)`) tiene, por construcción del propio lenguaje, un único dueño en todo momento — el compilador ya prohíbe que la tarea emisora lo siga usando después de enviarlo (documento 09, §2.1). Como nunca hay dos tareas con acceso simultáneo a ese valor, su contador de referencias puede manejarse con una operación **no atómica**, más barata — exactamente igual de rápido que mover un `Box` en Rust, sin el costo de sincronización de un `Arc`.
- Un valor **inmutable** compartido libremente entre tareas (capturado en varios `spawn`, documento 09 §1.1) sí puede tener referencias simultáneas desde distintos hilos, así que su contador (cuando el compilador decide representarlo con una referencia compartida en vez de copiarlo, ver §1) sí necesita ser atómico.

El propio sistema de tipos ya distingue estos dos casos (mutable-movido vs inmutable-compartido) desde el documento 09 — el modelo de memoria simplemente reutiliza esa misma distinción para decidir, caso por caso, si hace falta pagar el costo de atomicidad o no. El programador no elige esto explícitamente; es una consecuencia automática de las reglas de mutabilidad y concurrencia que ya existían.

## 3. Ciclos de referencias — acotados y con salida explícita

La debilidad clásica de ARC es que un ciclo de referencias (A apunta a B, B apunta a A) nunca llega a contador cero, y ambos quedan filtrados en memoria para siempre. En Ostrin, **un ciclo solo puede construirse mediante un campo `mut` reasignado después de la creación** — un dato inmutable se construye de una vez y no puede "apuntar hacia atrás" a algo creado después de él, porque nada en él puede cambiar tras construirse. Esto confina el problema exactamente a estructuras mutables con referencias cruzadas (listas doblemente enlazadas, árboles con puntero al padre, grafos generales) — un caso real pero minoritario, no el caso general del lenguaje.

Para esos casos, Ostrin ofrece una referencia débil explícita:

```ostrin
record Node {
    value: Int
    mut parent: weak<Node>
    mut children: List<Node>
}
```

- `weak<T>` no incrementa el contador de referencias del valor al que apunta — no lo mantiene vivo por sí sola.
- Para usar el valor, hay que "promoverla" explícitamente: `.upgrade() -> Option<T>`, que da `Some(valor)` si todavía existe (y sí incrementa el contador mientras se usa ese `Some`), o `None` si ya fue liberado por otro lado.

```ostrin
if let Some(p) = node.parent.upgrade() {
    print(p.value)
}
```

El caso común (la inmensa mayoría del código, sin estructuras cíclicas mutables) nunca necesita `weak<T>` ni pensar en esto.

## 4. Liberación determinista de recursos — `trait Drop`

Como ARC libera en el instante exacto en que el contador llega a cero (no en un momento futuro decidido por un recolector), Ostrin puede ofrecer limpieza determinista de recursos externos (archivos, sockets, buffers de GPU, conexiones) sin necesitar un mecanismo aparte tipo `with`/context manager:

```ostrin
record FileHandle {
    mut descriptor: Int
}

impl Drop for FileHandle {
    fn drop(self) {
        close_descriptor(self.descriptor)
    }
}

fn process() {
    handle = open_file("data.csv")
    read_all(handle)
}   // 'handle' sale de scope, contador llega a 0, 'drop' se ejecuta aquí mismo, el archivo se cierra
```

- `drop(self)` se ejecuta automáticamente en el momento exacto en que el contador de referencias de ese valor llega a cero — nunca hay que llamarlo a mano en el caso normal.
- **`drop` no puede fallar de forma que se propague**: su firma no permite devolver `Result` ni hacer `try` dentro — si una operación de limpieza puede fallar de verdad (ej. cerrar un archivo puede dar error de E/S), esa falla se registra/gestiona dentro del propio `drop` (por ejemplo, con un log), pero nunca se propaga como si fuera un error del código que dejó de usar el valor — ese código, en el punto en que el valor sale de scope, ya no está "haciendo" nada activamente que pueda fallar desde su perspectiva.
- Un valor puede implementar `Drop` sin tener ningún campo `mut` (por ejemplo, para invalidar un recurso externo referenciado read-only) — la condición para tener identidad y ARC es tener `Drop`, o tener algún campo `mut`, lo que ocurra primero.

## 5. Estructuras recursivas — boxing implícito

Un `enum` recursivo (`Tree<T>` del documento 05, con `Node(value: T, left: Tree<T>, right: Tree<T>)`) tendría, sin más, un tamaño infinito si cada variante se guardara "en línea" — un `Node` contiene un `Tree<T>` que puede volver a ser un `Node`, indefinidamente. El compilador inserta automáticamente una indirección al heap para los campos recursivos de un `enum` o `record` (equivalente conceptual a `Box` en Rust, pero sin que el programador lo escriba ni lo vea):

```ostrin
enum Tree<T> {
    Leaf
    Node(value: T, left: Tree<T>, right: Tree<T>)   // 'left'/'right' se asignan en el heap automáticamente
}
```

Esto es consistente con la decisión de no exponer un tipo `Box<T>` explícito (§ decisiones de fondo): el compilador ya necesita resolver este caso sin ayuda para que el lenguaje sea usable, así que no tiene sentido pedirle al programador que lo anote a mano.

## 6. `List<T>`, `String` y otros contenedores de tamaño dinámico

Cualquier contenedor cuyo tamaño no se conoce en compilación (`List<T>`, `String`, `Map`/`Set` cuando se diseñen) guarda su buffer de datos en el heap, independientemente de si su contenido es mutable o no — es una necesidad de representación, no una consecuencia de la regla de mutabilidad de §1. La diferencia con un `record` mutable es que el propio *binding* de una `List<T>` inmutable sigue comportándose como valor (documento 01): copiarlo conceptualmente copia la lista completa, aunque el compilador, sabiendo que es inmutable, puede compartir el mismo buffer entre varias copias sin que el programador lo note (mismo razonamiento de §1).

---

## 7. Preguntas abiertas para la siguiente sesión de diseño

1. **Elisión de incrementos/decrementos de ARC como optimización** (análisis de escape, evitar contar referencias cuando el compilador puede probar estáticamente que no hace falta) — es una decisión de implementación del compilador, no cambia la semántica descrita aquí, pero afecta directamente el rendimiento real.
2. **`Drop` y paneo durante el drop de una tarea que fue cancelada o que hizo panic** — interactúa con las preguntas abiertas de cancelación del documento 09.
3. ~~`dyn Trait`~~ — resuelto en el documento 15. Sigue abierto: `select` sobre canales.
