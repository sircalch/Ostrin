# Ostrin — Revisión de consistencia (documentos 01–09)

Versión: 0.1 — pasada de revisión general tras cerrar los 9 documentos core.

Este documento registra: (1) las inconsistencias/huecos encontrados al releer los 9 documentos de corrido y cómo se resolvieron (ya aplicado como ediciones a los documentos afectados), (2) una validación honesta de si el diseño mejora de verdad sobre lenguajes existentes, y (3) una evaluación de curva de aprendizaje.

---

## 1. Inconsistencias encontradas y corregidas

| # | Problema | Dónde | Resolución aplicada |
|---|----------|-------|----------------------|
| 1 | Doc02 §4 decía que capturar un `mut` en un closure requería tratarlo como "impuro" — pero el propio doc02 había cerrado como decisión de fondo que **no existe** distinción pure/impure. Contradicción directa dentro del mismo documento. | 02 §4 | Reescrito: closures normales (misma tarea) pueden capturar y mutar un `mut` libremente, como referencia compartida al mismo binding; cruzar un `spawn` es lo que lo prohíbe (documento 09), y esa prohibición no tiene nada que ver con pureza. |
| 2 | Dos sintaxis de lambda conviviendo sin que nadie lo decidiera: `fn(x) { ... }` (documento 02, usado en todos lados) vs `\|e\| { ... }` (solo en `catch`, documento 04). | 04 §2 | `catch \|e\| { }` → `catch fn(e) { }`. Una sola sintaxis de lambda en todo el lenguaje. |
| 3 | `self` se usaba sin tipo en cada método (`fn equals(self, other: Self)`) pese a que el documento 02 exige tipo explícito en **todo** parámetro, sin excepción declarada. | 02 / 03 | Se documenta explícitamente en 03 §1 que `self` es la única excepción reconocida: su tipo es siempre `Self`, implícito. |
| 4 | `Unit<D>` se usaba en un ejemplo (`convert(x, target: Unit<D>)`, documento 02) sin que el documento 01 hubiera introducido nunca "un símbolo de unidad solo, sin número, como valor". | 01 §2.2.1 (nueva) | Se añadió: un símbolo de unidad reconocido es, por sí mismo, una expresión de tipo `Unit<D>`. |
| 5 | No estaba definido si `Quantity<D>` guarda internamente `Int` o `Float` — ambigüedad real: `10 s / 3` con enteros ¿trunca o da fracción? | 01 §2.2 | Se fija: `Quantity<D>` siempre guarda `Float` internamente, sin importar cómo se vea el literal. Evita sorpresas de división entera en cantidades físicas. |
| 6 | La desestructuración de campos nombrados en `match` (`Circle(radius)`, `Node(value: _, left, right)`) se usaba con una forma abreviada nunca formalizada — funcionaba "por casualidad" porque los nombres de ejemplo coincidían. | 05 §2.4 | Se formalizó la regla: `Variant(campo)` es azúcar de `Variant(campo: campo)`; se puede renombrar con `campo: otro_nombre`, y mezclar formas en el mismo patrón. |
| 7 | La tabla de precedencia (documento 08) no incluía `to`/`until`/`step`, `^` (exponente de unidad) ni `as` — una tabla de precedencia incompleta es un bloqueador real para escribir el parser. Además, el primer borrador tenía el orden relativo de `within` y `to` **invertido** (`within` más fuerte que `to` haría que `x within 0 to 10` se leyera como `(x within 0) to 10`, no lo que se pretendía). | 08 §6 | Tabla completada y corregida: `to`/`until`/`step` ligan más fuerte que `within`/comparaciones, así el rango se construye antes de que `within` opere sobre él. |
| 8 | `spawn { }`, `spawn_scope { }`, `loop { }` usaban una forma de "bloque como último argumento" nunca formalizada como regla del lenguaje — parecía magia sintáctica limitada a esas tres palabras. | 02 §4.1 (nueva) | Formalizado como **regla general**: cualquier función cuyo último parámetro sea `fn(...) -> T` acepta un bloque final `{ }` en vez de ese argumento. `loop`/`spawn`/`spawn_scope` son casos de esta regla, no excepciones. Decisión confirmada contigo en esta sesión. |

### 1.1 Huecos identificados, no resueltos aún (quedan como pendientes explícitos)

Estos no se decidieron ahora porque exigirían diseño propio, no un ajuste de redacción:

- **Modelo de memoria**: ningún documento dice cómo se libera la memoria de un `record`, `List` o closure. Es, con diferencia, el hueco más importante — condiciona el diseño del compilador, si hace falta runtime con recolector de basura, y cómo interactúa con `spawn`/canales. **Acordamos que es el siguiente tema a diseñar a fondo.**
- **Literales de colección**: `[1, 2, 3]` para `List`, y la sintaxis de `Map`/`Set` (ya señalada como pendiente en documentos 06–09), nunca se definieron formalmente pese a usarse en varios ejemplos.
- **`dimension D`**: mencionado de pasada en 01 §3.5 (definir una dimensión nueva de usuario) sin sintaxis propia.
- Todos los pendientes ya listados al final de cada documento (sistema de paquetes, `derive`, herencia de traits, `dyn Trait`, cancelación de tareas, `select` sobre canales) siguen abiertos, sin cambios.

---

## 2. ¿El diseño realmente mejora sobre lo existente?

Evaluación honesta, no solo autocomplaciente, contra los lenguajes que la idea original de Ostrin señaló como referencia:

| Lenguaje | Problema señalado originalmente | ¿Ostrin lo resuelve? |
|---|---|---|
| Python | Errores descubiertos tarde | **Sí, de forma sólida.** Tipado estático + `match` exhaustivo + sin coerción implícita detectan en compilación clases enteras de errores que en Python solo aparecen en ejecución. |
| Python | Reproducibilidad / gestión de dependencias | **Parcial.** Módulos con visibilidad explícita ayudan a la estructura, pero el "Ostrin Package System" (versionado, lockfiles) sigue sin diseñar — es donde realmente se juega la reproducibilidad. |
| Rust | Complejidad de ownership/borrowing | **Sí, y es la aportación más fuerte del diseño hasta ahora.** La combinación "inmutable por defecto + mover solo al cruzar un canal/spawn" da seguridad de datos en concurrencia sin lifetimes ni un borrow checker general. Es una simplificación real, no cosmética. |
| Rust | Curva de aprendizaje general | **Parcial.** Se evitó la parte más dura de Rust (ownership), pero el sistema de tipos de Ostrin (genéricos de dimensión, trait bounds, `Result`/`Option`/`try`, pattern matching exhaustivo) sigue siendo un sistema de tipos "de lenguaje con tipado fuerte moderno" — comparable en carga conceptual a Rust o Swift, solo que mejor distribuida y con mejores mensajes de error modelados. No es trivial para alguien que nunca programó. |
| C/C++ | Seguridad de memoria | **No resuelto todavía** — es exactamente el hueco de memoria señalado arriba. Hasta que exista una respuesta (GC, ARC, o algo propio), esta promesa no está cumplida. |
| Julia | Tooling/ecosistema | Fuera del alcance de un documento de lenguaje — depende de inversión futura en stdlib/herramientas, no de decisiones de sintaxis. |
| Todos | "Cantidades físicas de primera clase" | **Sí, y es genuinamente diferenciador.** Ningún lenguaje mainstream tiene `Quantity<D>` como ciudadano de primera clase con inferencia de dimensión en operaciones (`m / s` → `Velocity` automáticamente) integrado al sistema de tipos, no como una librería externa (a diferencia de `uom` en Rust o `Unitful.jl`, que son bibliotecas, no parte del lenguaje). |

**Conclusión de esta sección**: el diseño cumple genuinamente su promesa más fuerte (unidades físicas nativas + concurrencia segura sin borrow checker). No cumple todavía la promesa de seguridad de memoria estilo Rust/C++ porque ese documento no existe aún — es la pieza que falta para poder decir "Ostrin es tan seguro como Rust, más simple de aprender".

## 3. ¿La curva de aprendizaje es manejable?

**A favor:**
- Las palabras en vez de símbolos crípticos (`try`, `to`/`until`, `and`/`or`/`not`, `within`, `approximately...tolerance`) hacen que el código se pueda leer en voz alta y tenga sentido para alguien que nunca vio Ostrin, incluso sin haber estudiado la sintaxis formalmente.
- El sistema de unidades es **opcional en la práctica**: nada obliga a usar `Quantity<D>` — se puede escribir `x = 5`, `y = 10`, `x + y` con `Int`/`Float` normales sin tocar el sistema de dimensiones nunca. Esto es importante y vale la pena documentarlo explícitamente en una futura guía de introducción: alguien puede aprender Ostrin como "un lenguaje normal" y descubrir las unidades más adelante, cuando las necesite.
- Solo dos niveles de visibilidad, sin sobrecarga de funciones, un solo tipo de lambda, un solo modelo de rangos: en varios puntos del diseño se sacrificó deliberadamente flexibilidad por simplicidad, y eso se nota a favor de la curva de aprendizaje.

**En contra / riesgo real:**
- El conjunto completo (genéricos de dimensión, trait bounds, `Result`/`Option` con `try`/`catch`, `match` exhaustivo, `spawn`/canales con la regla de "movido") es, sumado, una carga conceptual comparable a la de Rust o Swift — solo que mejor repartida y con mejores errores. **No es un lenguaji para un primer curso de programación día uno**, y no debería venderse como tal.
- El punto más delicado específicamente es el sistema de dimensiones genéricas (`<D: Dimension>`, documento 02 §3.2): es, en términos de sofisticación de tipos, comparable a features avanzadas de Haskell/Rust (type families / const generics). Para que no resulte "imposible" hace falta que el mensaje de error sea excelente (ya se modeló así en varios documentos, ej. OSTRIN-E1024) y que la documentación oficial dé permiso explícito para ignorar unidades al empezar.

**Recomendación concreta**: cuando se escriba la guía de introducción al lenguaje (fuera de alcance de estos documentos de diseño core), estructurarla en niveles explícitos — "Ostrin básico" (variables, funciones, `if`/`match`, sin unidades ni concurrencia) → "Ostrin con unidades" → "Ostrin concurrente" — en vez de presentar las nueve piezas de golpe como se hizo en esta sesión de diseño.

---

## 4. Siguiente paso acordado (cumplido)

Diseñar el **modelo de memoria** (documento 11): cómo se gestiona la vida de los valores en el heap, si existe un recolector de basura, y cómo encaja con la regla de movimiento ya definida en concurrencia (documento 09). Completado — y de ahí se derivó la segunda pasada de revisión (§5).

---

## 5. Segunda pasada de revisión (tras los documentos 09–14)

### 5.1 El hallazgo más importante: dos reglas de mutación incompatibles

Al releer `Iterator`/`Fibonacci` (documento 06) junto con `List.push()` (documento 13), aparecieron **dos reglas distintas y contradictorias** para "cuándo se puede llamar un método que muta el receptor":

- Documento 06 decía que `next(self)` en `Fibonacci` podía mutar sus campos sin que el binding que lo llamaba (dentro de un `for`) tuviera que ser `mut` — la mutabilidad quedaba "encapsulada" en el `impl`.
- Documento 13 decía que `.push()` sobre un `List` **sí** exige que el binding sea `mut`, si no es error de compilación.

Aplicadas literalmente, estas dos reglas son incompatibles entre sí — no había ninguna razón declarada por la que un `record` con campos `mut` se comportara distinto de un `List`. Esto no es un detalle cosmético: es exactamente el tipo de ambigüedad que socavaría la promesa central del lenguaje ("inmutable por defecto" garantiza que nada cambia un valor sin que el programador lo permita explícitamente) si un método pudiera mutar un valor por debajo aunque el binding fuera inmutable.

**Resolución aplicada** (documento 03, §1.1, nueva): se formalizó la distinción `self` (solo lectura) vs `mut self` (mutable) como forma de declarar un método, tomando la idea de `&self`/`&mut self` de Rust pero sin lifetimes ni borrow checking — solo una marca binaria:

- Un método `mut self` solo puede llamarse a través de un binding `mut`, desde dentro de otro método `mut self` del mismo valor, o sobre un valor recién construido sin otro dueño todavía (este último caso es justo el que salva el ejemplo de `Fibonacci`: el iterador que un `for` obtiene de `.iterator()` es un valor nuevo, exclusivo del propio `for`, así que puede tratarse como mutable aunque la colección original no lo sea — documento 06, §3, actualizado).
- `List.push()`/`Map.set()`/`Set.add()` son métodos `mut self` de la stdlib — de ahí que exijan un binding `mut`, con la misma regla general, no un caso especial de las colecciones.

Esto también dejó ver que el ejemplo de `Fibonacci` implementaba `Iterator<Int>` directamente en vez de `Iterable<Int>` sin que el documento lo explicara — se formalizó como una forma válida más: un tipo puede ser su propio iterador cuando la secuencia es inherentemente de un solo paso.

### 5.2 Otras correcciones de esta pasada

- Un código de error duplicado: `OSTRIN-E1052` se usaba tanto para "colisión de `derive` con `impl` manual" (documento 12) como para "llamar un método mutable sobre binding inmutable" (documento 13, versión anterior de `.push()`). Se reasignó el segundo caso a `OSTRIN-E1053`, coherente además con el nuevo error general de `mut self` (documento 03, §1.1).
- Una frase obsoleta en el documento 03 seguía mencionando `derive Eq` con la sintaxis antigua (previa a que el documento 12 fijara `record Tipo: Eq { ... }`) — corregida.
- **Barrido general de listas de "preguntas abiertas"**: varios documentos (02, 04, 05, 06, 07, 08, 09, 11, 12) seguían citando como pendientes temas que ya se habían cerrado en documentos posteriores (`derive`, módulos/visibilidad, `Map`/`Set`, sistema de paquetes, concurrencia, sistema de lógica/expresiones, `Hash`, rangos) — cada mención se tachó o marcó como resuelta con referencia cruzada al documento que la cerró. Sin este barrido, alguien que abriera un documento antiguo por separado se llevaría información falsa sobre qué falta por diseñar.

### 5.3 Qué queda genuinamente pendiente tras esta pasada

Con el barrido hecho, la lista real de pendientes se reduce a: `as D` (conversión genérica de escalar a cantidad, documento 02 §3.3), `dyn Trait` (polimorfismo dinámico, documento 03 §7), `select` sobre canales y cancelación de tareas (documento 09 §5), elisión de ARC como optimización (documento 11 §7), operadores bit a bit y ordenación general (documento 13 §6), y — del documento 14 — índice de descubrimiento opcional, verificación de integridad, y workspaces.

*(Actualización: `dyn Trait` se cerró en el documento 15, y `as D` en el documento 16 — resultó no necesitar ningún mecanismo nuevo, ver §6 más abajo.)*

---

## 6. Tercera pasada de revisión (tras los documentos 15–16, cierre del núcleo del lenguaje)

Con `dyn Trait` y `as D` cerrados, los 16 documentos cubren el núcleo completo planteado al inicio de esta sesión: variables, tipos, unidades, funciones, traits, errores, enums, iteradores, módulos, lógica, concurrencia, memoria, `derive`, colecciones, paquetes y polimorfismo dinámico. Esta pasada final se concentró en dos clases de error que ya habían aparecido antes y que solo se detectan mirando el conjunto completo, no un documento a la vez.

### 6.1 Colisión de palabra reservada `unit`

El ejemplo genérico de `as` añadido al documento 01 (§3.3, y repetido en el documento 16) usaba `unit` como nombre de parámetro de función — pero `unit` es palabra reservada para declarar unidades nuevas (`unit USD : Currency`, documento 01 §3.5). Un lenguaje no puede aceptar la misma palabra como keyword y como identificador libre sin reglas de contexto adicionales que Ostrin no ha decidido tener. Corregido: el parámetro se renombró a `target_unit` en ambos documentos.

### 6.2 Registro consolidado de códigos de error y palabras reservadas

Ya van dos colisiones detectadas en pasadas distintas de esta revisión (`E1052` reusado entre documentos 12 y 13; `unit` usado como nombre de parámetro pese a ser palabra reservada). **Ambos registros se movieron a su propio documento**: [17-referencia-del-lenguaje.md](17-referencia-del-lenguaje.md), §2 (palabras reservadas) y §5 (códigos de error) — para no seguir duplicando estas tablas cada vez que se revisa el conjunto, que es precisamente lo que causó las colisiones en primer lugar.

### 6.3 Recomendación para lo que sigue (cumplida)

El núcleo del lenguaje quedó completo y consistente. Se consolidó la **gramática formal (EBNF)** completa, junto con las tablas de errores y palabras reservadas, en el documento 17 — es la referencia que hace falta antes de escribir el lexer/parser del compilador.

---

## 7. Lo que la implementación encontró que la revisión de documentos no vio

Empezar el compilador real (Rust, `compiler/`) en vez de seguir solo con documentos de diseño encontró, en horas, problemas que tres pasadas de revisión de texto no habían visto — la validación más fuerte de que "revisar" y "construir" son verificaciones distintas y complementarias:

- **`in` nunca se añadió a las palabras reservadas** (documento 17) pese a usarse en `for x in ...` desde el documento 06 — lo detectó el lexer al tokenizar el primer ejemplo real.
- **Terminación de sentencias sin `;` nunca se diseñó formalmente**: el parser fusionaba incorrectamente el final de una sentencia con el principio de la siguiente cuando esta empezaba con `-`, `(` o `[` — la misma clase de bug que hizo famoso el ASI de JavaScript. Se decidió (con tu confirmación) que los saltos de línea son significativos, con reglas explícitas acotadas a esos tres tokens ambiguos; ahora formalizado en el documento 17, §1.6.
- El propio verificador de tipos, al implementarse, confirmó que las reglas de dimensiones del documento 01 (incluida la unificación de `<D: Dimension>` en llamadas a función) funcionan exactamente como se diseñaron: los cuatro errores de un archivo de prueba deliberadamente incorrecto (`E1024`, `E1025`, `E1001`, y de nuevo `E1024` a través de una llamada genérica) se detectaron correctamente.

Conclusión práctica: a partir de ahora, cualquier documento de diseño nuevo debería, cuando sea razonable, probarse contra el compiler real (aunque sea con un ejemplo pequeño) antes de darse por cerrado — es un nivel de validación que ninguna cantidad de relectura reemplaza.
