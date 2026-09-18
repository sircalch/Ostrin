# Ostrin — Contexto completo del proyecto (para retomar en otra herramienta)

Este documento existe para que puedas seguir trabajando en Ostrin desde otra sesión/herramienta (Codex u otra) sin perder el hilo. Resume: qué es Ostrin, por qué existe, qué se decidió y por qué, qué se construyó, qué está probado, y qué falta.

---

## 1. Qué es Ostrin y por qué existe

Ostrin es un lenguaje de programación de propósito general diseñado desde cero (no es un wrapper sobre otro lenguaje) cuya identidad central es:

1. **Cantidades físicas de primera clase en el sistema de tipos.** `temperature = 310 K` no es un `Float` con una unidad pegada — es un tipo `Quantity<Temperature>`, y el compilador verifica dimensiones (`5 nm + 10 s` es un error de compilación, no de ejecución), incluida la composición automática (`Length / Time` → una velocidad).
2. **Seguridad de concurrencia sin borrow checker.** La idea central: como los bindings son *inmutables por defecto*, compartir datos entre tareas concurrentes ya es seguro sin necesitar el sistema de ownership/lifetimes de Rust. Solo lo que puede cambiar (`mut`) necesita una regla especial, y esa regla es simple: no se puede capturar `mut` en una tarea (`spawn`), tiene que viajar por un canal.
3. **Legibilidad por encima de brevedad simbólica.** Donde un símbolo se presta a error o a mala lectura, se usa una palabra: `try` en vez de `?`, `and`/`or`/`not` en vez de `&&`/`||`/`!`, `to`/`until` en vez de `..`/`..=`. Donde el símbolo es notación matemática universal sin ambigüedad (`+`, `==`, `<`), se mantiene el símbolo.
4. **Todo lo diferenciador pasa por el mismo mecanismo, no hay casos especiales.** Los operadores de `Quantity<D>` están implementados con los mismos traits (`Add`, `Eq`, `Ord`) que usaría cualquier tipo de usuario — no hay "magia" reservada para tipos incorporados.

El objetivo explícito (dicho por el usuario al principio de la sesión): que Ostrin tenga una razón técnica real para existir, no ser "otra sintaxis bonita sobre Python/Rust". El plan siempre fue: (1) diseñar el lenguaje completo primero, (2) implementar un compilador real que valide que el diseño funciona, empezando en Rust como lenguaje de implementación de la fase 1 (con la idea, no ejecutada todavía, de que Ostrin se autohospede más adelante).

---

## 2. Estado general (resumen ejecutivo)

- **17 documentos de diseño** completos y revisados (tres pasadas de consistencia formales), cubriendo el núcleo entero del lenguaje.
- **Un compilador/intérprete real en Rust** (~4000+ líneas), en `compiler/`, que compila con `cargo build` y corre con `cargo run -- <flags> archivo.ostrin`.
- **65 pruebas automatizadas** (`cargo test`) que verifican comportamiento exacto (no solo "no truena") sobre las piezas centrales del lenguaje.
- **68 programas/archivos de ejemplo reales** en `examples/`, incluidos varios proyectos multi-archivo.
- Sigue siendo un **prototipo de validación de diseño**, no un lenguaje listo para producción: es un intérprete que recorre el AST (no genera código máquina), sin paralelismo real de sistema operativo, con una stdlib inicial de E/S, sin LSP completo.

**Lo más importante que hay que entender**: cada decisión de diseño de los 17 documentos fue *implementada y probada con un programa real*, no solo escrita en el papel. Varias veces, construir el compilador encontró bugs de diseño que ninguna revisión de texto había visto (ver sección 6).

---

## 3. Los 17 documentos de diseño (`docs/design/`)

Cada uno tiene una sección "Decisiones de fondo ya cerradas" al principio y "Preguntas abiertas" al final — son la fuente de verdad del *por qué*. Resumen de cada uno:

| # | Documento | Decisión central |
|---|---|---|
| 01 | `variables-tipos-unidades.md` | Bindings inmutables por defecto (`mut` explícito para lo contrario); tipado estático con inferencia; `Quantity<D>` como tipo de primera clase con vector de exponentes sobre 9 dimensiones base (7 SI + Currency + Information); reglas aritméticas completas (sumar exige misma dimensión, multiplicar/dividir compone dimensiones); `as <unidad>` para inyectar unidad en un escalar, generalizado a aceptar cualquier `Unit<D>` (variable o literal). |
| 02 | `funciones-y-firmas.md` | Tipo de retorno explícito obligatorio; sin distinción pure/impure; argumentos con nombre y valores por defecto; dos tipos de genérico — `<T>` normal y `<D: Dimension>` (el diferenciador real); closures; **bloques finales / trailing closures** como regla general del lenguaje (`f(x) { ... }` ≡ `f(x, fn() { ... })`), no solo para `loop`/`spawn`. |
| 03 | `traits.md` | Traits nominales explícitos (`impl Trait for Type`, no estructural); métodos con default; **operadores sobrecargados vía traits estándar** (`Add`, `Eq`, `Ord`), igual para tipos de usuario que para `Quantity<D>`; regla de coherencia tipo "orphan rule"; `self` vs `mut self` como las dos formas de receptor de método (añadido en revisión de consistencia — ver §6). |
| 04 | `errores-y-result.md` | `Option`/`Result` como enums del núcleo del lenguaje; propagación con **`try` como prefijo** (no `?` como en Rust, decisión deliberada de legibilidad); `catch` explícito para convertir tipos de error (nada de conversión automática tipo `From` de Rust); panics para bugs de programador, `Result` para fallos esperables del dominio. |
| 05 | `enum-y-pattern-matching.md` | `enum` con variantes con datos (posicionales o nombrados); `match` como expresión, exhaustivo obligatorio; guards; patrones anidados; azúcar de campo (`Circle(radius)` liga por nombre de campo); `if let`. |
| 06 | `rangos-e-iteradores.md` | Rangos con palabras (`to` inclusivo, `until` exclusivo, `step`), no símbolos; `for`/`while`/`loop` (`loop` es expresión, con `break valor`); protocolo `Iterator`/`Iterable` vía traits — la única excepción a "inmutable por defecto" (`next(mut self)`); combinadores `.map`/`.filter`/`.fold`/`.find`/`.any`/`.all`. |
| 07 | `modulos-y-visibilidad.md` | Un archivo = un módulo; privado por defecto, `pub` explícito; `import` calificado por defecto, con alias y nombres explícitos; sin wildcard import; `pub import` para re-exportar; **ciclos de importación prohibidos** (grafo debe ser DAG). |
| 08 | `logica-y-expresiones.md` | Comparación con símbolos (`==`, `<`), lógicos con palabras (`and`/`or`/`not`); sin comparaciones encadenadas (se usa `within` en su lugar); `if`/`else` como expresión (no hace falta ternario); **`within`** (pertenencia a rango) y **`approximately ... tolerance ...`** (igualdad con margen, tolerancia obligatoria sin default); tabla de precedencia completa. |
| 09 | `concurrencia.md` | **La pieza de identidad más fuerte del lenguaje.** `spawn`/`spawn_scope`/`channel` estilo CSP; nada de borrow checker — solo una regla: no se puede capturar `mut` en `spawn` (error E1100), y enviar un valor `mut` por un canal lo mueve (no se puede reusar después, error E1101). Todo lo demás (datos inmutables) se comparte libremente entre tareas sin restricción. |
| 10 | `revision-de-consistencia.md` | Bitácora de 3 pasadas de revisión — documenta bugs de diseño reales encontrados (ver §6) y, tras empezar la implementación, una 4ª sección con lecciones de "la implementación encuentra lo que la relectura no ve". |
| 11 | `modelo-de-memoria.md` | **ARC** (conteo de referencias), no GC de trazado — elegido por liberación determinista (relevante para cerrar archivos/recursos). Identidad de referencia solo donde hay mutabilidad (mismo principio que concurrencia). `weak<T>` para los pocos casos de ciclos (que solo pueden formarse vía campos `mut`). `trait Drop`. Sin `Box<T>` explícito — el compilador decide representación. |
| 12 | `derive-y-herencia-de-traits.md` | Herencia de traits (`trait Ord: Eq`); **`derive`** reutilizando la misma sintaxis `Tipo: Trait, Trait { ... }` de los supertraits — genera el `impl` real, no rompe la regla de "nominal explícito". Conjunto derivable: `Eq`, `Ord`, `Printable`, `Default`, `Hash`. |
| 13 | `map-set-y-literales-de-coleccion.md` | `List` (`[1,2,3]`), `Map` (`["k": v]`, mismo corchete que `List`, diferenciado por `:`), `Set` (`{1,2,3}`, distinguible de un bloque porque un bloque nunca tiene comas a nivel superior) — evitando la ambigüedad de Python con `{}`. `trait Hash` nuevo. Mutación (`.push`/`.set`/`.add`) exige binding `mut`. |
| 14 | `sistema-de-paquetes.md` | `ostrin.toml` (formato TOML, no Ostrin nativo); **descentralizado** (dependencias por URL de Git, sin registro central, como Go); SemVer; `ostrin.lock` fija el **commit exacto**, no el tag (porque un tag se puede re-apuntar). |
| 15 | `dyn-trait.md` | Polimorfismo dinámico como mecanismo de escape explícito (no el modo por defecto): `List<dyn Shape>` para colecciones heterogéneas de tipos que comparten un trait pero no un `enum` cerrado. Reglas de "object safety". Boxing implícito, reutilizando el mecanismo ya existente para enums recursivos. |
| 16 | `as-generico.md` | Cierre de un pendiente: resultó que no hacía falta ningún mecanismo nuevo — una regla aritmética que faltaba (`Quantity / Int` conserva dimensión) y la generalización de `as` a cualquier `Unit<D>` (no solo literales) ya resolvían el caso. |
| 17 | `referencia-del-lenguaje.md` | **Consolidación única** de: léxico formal (comentarios, literales, `_` como separador de miles), palabras reservadas, gramática EBNF completa, tabla de precedencia canónica, y registro de todos los códigos de error/advertencia — para dejar de tener estas tablas duplicadas en varios documentos (lo cual ya había causado colisiones reales, ver §6). |

---

## 4. El compilador (`compiler/`, Rust, sin dependencias salvo `toml`)

Proyecto Cargo normal: `cd compiler && cargo build`, binario `ostrinc`.

```
compiler/
├── Cargo.toml              (una sola dependencia externa: `toml`, para ostrin.toml)
├── src/
│   ├── main.rs              — CLI: --check, --tokens, --ast, --run, --json, --symbols, --members, --help, --version
│   ├── lexer/
│   │   ├── mod.rs           — tokenizador, trackea saltos de línea (newline_before) por token
│   │   └── token.rs         — TokenKind, tabla de palabras reservadas
│   ├── parser/mod.rs        — descenso recursivo + precedencia por niveles; ~900 líneas
│   ├── ast.rs                — todos los nodos del AST (Item, Stmt, Expr, Pattern, Type, ...)
│   ├── types.rs              — Dimension (HashMap<String,i32>), registro de unidades stdlib, Ty (tipos semánticos)
│   ├── typeck/mod.rs         — verificador de tipos: dimensiones, generics `<T: Trait>`, llamadas/constructores e impls genéricos, especialización de firmas de traits aplicados, patrones/E1061, mut/E1001/E1042/E1050/E1053/E1054/E1055/E1056/E1057/E1060, E1100 (análisis de variables libres para spawn)
│   ├── interpreter/mod.rs    — árbol de sintaxis → ejecución: Value, Env (Rc<RefCell<>>), records/enum/match/impl/spawn/channel/List/Map/Set
│   ├── modules.rs            — carga multi-archivo: descubrimiento, ciclos (E1081), visibilidad (E1080), reescritura de AST (mangling de nombres cruzando módulos)
│   └── package.rs            — ostrin.toml, dependencias `path` (funcionan) y `git` (reconocidas, rechazadas explícitamente sin red)
└── tests/
    └── examples.rs           — 65 pruebas de integración (invocan el binario compilado, comparan stdout/stderr exacto)
```

### Cómo correrlo

```bash
cd compiler
cargo build
cargo test                                    # 65 pruebas, deben pasar todas
./target/debug/ostrinc archivo.ostrin         # solo verifica tipos
./target/debug/ostrinc --run archivo.ostrin   # verifica y ejecuta
./target/debug/ostrinc --ast archivo.ostrin   # imprime el AST
./target/debug/ostrinc --tokens archivo.ostrin # imprime los tokens
./target/debug/ostrinc --check --json archivo.ostrin # JSON Lines para editores
./target/debug/ostrinc --symbols --json archivo.ostrin # símbolos y firmas
./target/debug/ostrinc --members --json archivo.ostrin # miembros por tipo y bindings locales
```

En Windows, si `cargo`/`rustc` no están en el PATH de la sesión: `$env:Path += ";$env:USERPROFILE\.cargo\bin"` (rustup se instaló vía `winget install Rustlang.Rustup` durante esta sesión).

### Arquitectura de evaluación (para quien continúe el intérprete)

- **`Value`** (en `interpreter/mod.rs`): `Int, Float, Bool, Char, String, Quantity(f64, Dimension, unit_str), List(Rc<RefCell<Vec<Value>>>), Closure, Record(String, Rc<RefCell<Vec<(String,Value)>>>), EnumInstance(enum, variant, HashMap<String,Value>), Task, Channel, Map(Rc<RefCell<Vec<(K,V)>>>), Set(Rc<RefCell<Vec<Value>>>), Void`.
- **Mutabilidad/identidad**: `List`, `Record`, `Map`, `Set`, `Task` y `Channel` usan `Rc<RefCell<>>` cuando necesitan identidad mutable. `List.push(value)` y `List.remove_at(index)` modifican el storage compartido; un alias puede observar el cambio. El type-checker exige binding `mut` para `push`/`remove_at` (E1053), y también para `Map.set`/`Map.remove` y `Set.add`/`Set.remove`.
- **Errores de control de flujo** (`return`/`break`/`continue`) se implementan reutilizando el canal `Result<Value, RuntimeError>` de Rust — `RuntimeError::Return(v)` sube por `?` de forma natural hasta el punto que sabe capturarlo (llamada de función para `Return`, bucle para `Break`/`Continue`). Patrón limpio, vale la pena mantenerlo si se sigue extendiendo.
- **Operadores sobre tipos de usuario** pasan por `Interpreter::eval_binary`, que busca `impl` real vía `find_method`, y si no hay `impl` pero el tipo tiene `derive(Eq)`/`derive(Ord)`, sintetiza la comparación campo por campo usando el orden de declaración (`self.records[type_name].fields`). Los tipos incorporados (`Int`, `Quantity`, etc.) tienen una vía rápida aparte (`eval_binary_builtin`) que **no** pasa por traits — es una simplificación deliberada del intérprete, no lo que dicta el diseño.
- **Módulos**: `modules.rs` resuelve todo el grafo de imports, y luego hace una **pasada de reescritura de AST** que renombra cada símbolo declarado fuera del módulo de entrada con un prefijo (`modulo.path::nombre`) y reescribe cada referencia cruzada (`alias.miembro`) para apuntar directamente ahí — es, en efecto, un mini-linker.
- **Coherencia de traits entre módulos**: durante esa reescritura se conserva `module_path` en records, enums, traits e `impl`; los traits públicos también participan en imports explícitos. Esto permite que el type-checker aplique la regla `orphan` con el módulo real de definición, no con una suposición posterior.

---

## 5. Qué está probado (y cómo verificarlo)

`compiler/tests/examples.rs` tiene 65 pruebas. Cubren, con valores exactos esperados (no solo "no falla"):

- Aritmética de `Quantity<D>` con conversión de unidades real (`5 nm + 2 m`, `velocity(10 m, 2 s)`, cancelación dimensional `2m/5nm = 400000000`).
- Los 4 errores deliberados de dimensión/mutabilidad (`E1024`, `E1025`, `E1001`) en un mismo archivo.
- Fibonacci vía protocolo `Iterator` (secuencia completa).
- Traits reales (`impl Add`, `impl Eq`, `impl Ord` con `Ordering`) y `derive(Eq, Ord)` sin `impl` manual.
- `dyn Trait` con lista heterogénea de formas, despacho polimórfico.
- `spawn`/`channel` (productor/consumidor + `Task.join()`), y los dos errores de seguridad de concurrencia (`E1100` estático, movido-tras-enviar en tiempo de ejecución).
- `List` mutable con `.push()`/`.remove_at()`, aliasing observable e intento inválido sobre binding inmutable (`E1053`); `Map`/`Set` reales con dedup y comparación vía `impl Eq`; combinadores `.map/.filter/.fold/.find/.any/.all`.
- Exhaustividad de `match` sobre enums de usuario (`E1060`), incluyendo variantes sin datos, wildcard y el hecho de que un `guard` no cuenta como cobertura total.
- Generics normales: inferencia de `T` en llamadas (`List<T>` incluido), validación de métodos ofrecidos por un trait bound y rechazo de tipos sin la implementación nominal requerida (`E1042`).
- Generics explícitos: llamadas de funciones, métodos con trait bound, constructores de enums/records e `impl<T>` con aridad comprobada.
- Traits nominales: supertraits transitivos, métodos `default` heredados en runtime (incluidas cadenas de varios niveles), métodos requeridos, firmas de métodos de `impl`, especialización de argumentos de traits (`Convert<Int>`), despacho selectivo de impls aplicados en records, enums y `Quantity` (`Box<Int>`, `Item<String>`, `Quantity<Time>`), regla `orphan` y colisiones heredadas (`E1050`, `E1054`–`E1057`).
- Patrones de `match`: constructores anidados, campos posicionales/nombrados, bindings con el tipo del campo y errores de aridad/nombre/tipo (`E1061`).
- Enums y records genéricos aplicados: inferencia de `Maybe<T>`, `Outcome<T,E>` y `Pair<T>`, patrones internos completos y detección de combinaciones faltantes (`E1060`).
- Módulos multi-archivo: import calificado/alias/nombres explícitos, visibilidad (`E1080`), ciclos (`E1081`).
- Recuperación de errores del parser: 3 errores de sintaxis independientes reportados en una sola pasada.
- Sistema de paquetes: dependencia `path` resuelta y ejecutada de verdad; dependencia `git` rechazada con mensaje claro (sin red).

Todos los ejemplos usados están en `examples/*.ostrin` y `examples/proj*/`, `examples/pkg_*` — son la mejor forma de ver Ostrin "en carne" con casos reales.

---

## 6. Bugs de diseño reales que la implementación encontró (que ninguna revisión de texto vio)

Vale la pena que quien continúe sepa que este patrón se repitió varias veces — **construir código real encuentra huecos que releer documentos no encuentra**:

1. `in` (usado en `for x in ...` desde el primer documento) nunca se había añadido a la lista de palabras reservadas — el lexer lo tokenizaba como identificador normal.
2. **Terminación de sentencias sin `;`**: nunca se diseñó formalmente cómo se sabe dónde termina una sentencia. El parser fusionaba mal el final de una sentencia con el inicio de la siguiente cuando esta empezaba con `-`, `(` o `[` (la misma clase de bug que hizo famoso el ASI de JavaScript). Se resolvió con **saltos de línea significativos**, reglas explícitas acotadas a esos tres tokens ambiguos (documento 17, §1.6).
3. **Contradicción real sobre mutabilidad**: el ejemplo de `Fibonacci` (doc 06) dejaba mutar sin exigir `mut`, pero `List.push()` (doc 13) sí lo exigía. Se resolvió formalizando `self` vs `mut self` como las dos formas de receptor de método (documento 03, §1.1) — una distinción que no existía antes de implementar.
4. `Self` como tipo de parámetro (`other: Self`) no se reconocía en el parser de tipos — se lexa como palabra clave distinta de un identificador normal.
5. `trait Foo { ... }` nunca se había implementado en el parser (solo `impl`) — cualquier archivo que declarara un trait explícitamente fallaba.
6. Un código de error duplicado (`E1052` usado dos veces con significados distintos) y una colisión de palabra reservada (`unit` usado como nombre de variable en un ejemplo).
7. El campo `average` del documento 02 usaba una construcción (`as D`) que además de no existir, habría sido *semánticamente incorrecta* si hubiera compilado (habría cancelado la dimensión a `Float`, perdiendo la unidad del promedio).

**Lección para quien continúe**: cuando agregues una feature nueva al diseño, trata de escribir un ejemplo `.ostrin` real y correrlo por el compilador antes de darla por cerrada. Es la única forma de validación que de verdad funciona.

---

## 7. Limitaciones conocidas y explícitamente documentadas (no las repitas por descuido — ya están anotadas)

- **Sin paralelismo real de SO**: `spawn` corre el bloque de forma síncrona e inmediata. Las reglas de seguridad (E1100, movido-tras-enviar) sí son reales, pero no hay hilos de verdad. Convertirlo requeriría reescribir `Value`/`Env` de `Rc<RefCell<>>` a `Arc<Mutex<>>` en todo el intérprete — refactor grande, decidido explícitamente NO hacer por ahora (ver conversación: se preguntó al usuario y eligió la simulación).
- **Los tipos incorporados no pasan por traits en el intérprete** (`Quantity`, `Int`, etc. tienen su propia vía rápida de operadores) — el diseño (doc03 §4) dice que deberían, pero cambiarlo no aporta nada observable ahora.
- **`derive(Ord)` solo para `record`**, no para `enum` de usuario (sí funciona para `Ordering` porque está hardcodeado). Requeriría el orden de declaración de variantes.
- **Traits con alcance deliberadamente acotado** — ahora se verifican supertraits transitivos, métodos requeridos, defaults en runtime (también en cadenas de varios niveles cuando sus `impl` ancestrales existen), firmas de métodos de `impl`, especialización de firmas con argumentos del trait, despacho aplicado para records, enums y dimensiones base de `Quantity`, regla `orphan` y colisiones heredadas; siguen pendientes la validación semántica de cuerpos `default` y los bounds completos de todas las formas genéricas.
- **El reescritor de módulos no distingue shadowing**: si una variable local casualmente comparte nombre con un símbolo importado, gana el import (caso rarísimo, documentado).
- **Sin dependencias `git` reales** (ver arriba) — decisión de seguridad, no de tiempo.
- **Generics normales con alcance deliberadamente acotado** — el checker ya infiere parámetros simples, contenedores y `enum`/`record` aplicados (`List<T>`, `Maybe<T>`, `Pair<T>`), acepta argumentos explícitos en funciones, métodos y constructores, valida bounds nominales (`E1042`), sustituye argumentos de traits al comparar firmas de `impl` y el intérprete despacha records, enums y `Quantity` por aplicación; aún falta la sustitución en todos los cuerpos especializados y el chequeo estático de métodos concretos.
- **La exhaustividad de `match` y sus formas básicas ya se validan para variantes conocidas, campos posicionales/nombrados, anidamiento y combinaciones finitas de enums aplicados; todavía hay un límite conservador para estructuras recursivas profundas y toda la semántica de tipos de patrones avanzada descrita en doc05.**
- **Field order de `Map`/`EnumInstance` no garantizado** (usan `HashMap` internamente) — a diferencia de `Record`, que sí se corrigió para preservar orden de declaración (necesario para que `derive(Ord)` funcione correctamente).

---

## 8. Qué se recomienda para continuar (en orden sugerido, pero es solo una sugerencia)

1. Completar la validación de bounds y sustituciones dentro de todos los `impl` especializados, incluidos cuerpos `default` y argumentos del trait/tipo.
2. Añadir resolución estática de métodos sobre tipos concretos; actualmente los métodos concretos no cubiertos por un trait bound dependen del despacho runtime.
3. Si se quiere paralelismo real: el refactor `Rc<RefCell<>>` → `Arc<Mutex<>>` es grande pero mecánico — hacerlo solo si de verdad hace falta demostrar concurrencia real, no antes.
4. Backend de compilación real (LLVM u otro) — el salto de "intérprete" a "compilador" de verdad. Es el trabajo más grande de todos los pendientes.

**Regla de oro para seguir**: cualquier decisión de diseño nueva, probarla con un archivo `.ostrin` real corriendo por `cargo test` antes de darla por cerrada — es lo que ha mantenido este proyecto honesto durante toda la sesión.

---

## 9. Filosofía de trabajo en esta sesión (para mantener el mismo estilo)

- Cuando hay una bifurcación de diseño genuina (no solo una decisión mecánica), se le preguntó al usuario con opciones concretas y una recomendación — nunca se asumió en silencio en temas de fondo.
- Cuando una decisión era una extensión natural y de bajo riesgo de algo ya decidido (ej. completar una tabla de precedencia, nombrar un error), se decidió directamente y se explicó el porqué, sin generar fricción innecesaria.
- Cada fase de implementación terminó con: compilar limpio (sin warnings), correr contra un ejemplo nuevo diseñado para esa feature, y una regresión completa contra TODOS los ejemplos anteriores antes de darse por buena.
- Nunca se hicieron operaciones de red no solicitadas (el caso de las dependencias `git` es el ejemplo más claro: se implementó todo lo que se podía hacer sin red, y se rechazó explícitamente lo que la hubiera requerido).

---

## 10. Bitácora de continuación — 2026-09-17

Esta sección registra el trabajo realizado en la sesión actual para que otra
herramienta pueda retomarlo directamente.

### Punto de partida

- Se leyó este documento completo y se tomó la primera recomendación pendiente
  (identidad mutable de `List`) como el siguiente paso natural.
- La línea base estaba sana: `cd compiler && cargo test` compilaba limpio y
  pasaban las 20 pruebas existentes.
- El directorio no es un repositorio Git (`git status` devolvió que no hay un
  repositorio en la ruta); los cambios quedan documentados aquí y en los
  archivos modificados.

### Cambios realizados

1. `compiler/src/interpreter/mod.rs`
   - Cambié `Value::List(Vec<Value>)` a
     `Value::List(Rc<RefCell<Vec<Value>>>)`, siguiendo el patrón de `Map` y
     `Set`.
   - Actualicé display, indexado, `for`, `sum`, `length` y los combinadores
     para leer snapshots del storage sin mantener un borrow durante callbacks.
   - Implementé `List.push(value) -> Void`.
   - Implementé `List.remove_at(index) -> T`; devuelve el elemento quitado y
     reporta error de ejecución para índices negativos o fuera de rango.
   - Las listas producidas por `map`, `filter`, `Map.keys()` y `Map.values()`
     también usan el nuevo storage compartido.

2. `compiler/src/types.rs` y `compiler/src/typeck/mod.rs`
   - Añadí `Ty::Map` y `Ty::Set` y mejoré la inferencia de literales de esas
     colecciones.
   - Añadí los tipos de retorno conocidos de las operaciones de colecciones.
   - Implementé `E1053` para impedir métodos mutables sobre bindings
     inmutables: `List.push`/`remove_at`, `Map.set`/`remove` y
     `Set.add`/`remove`.

3. `examples/list_mutation.ostrin` y `examples/list_mutation_error.ostrin`
   - Añadí un ejemplo ejecutable de `push`, `remove_at`, aliasing y `length`.
   - Añadí un ejemplo negativo para comprobar `E1053`.

4. `compiler/tests/examples.rs`
   - Añadí dos pruebas de integración; la suite pasó de 20 a 22 pruebas.

5. `docs/design/13-map-set-y-literales-de-coleccion.md`
   - Fijé la firma y semántica de `List.remove_at`: devuelve el elemento
     retirado y falla sin modificar la lista si el índice no es válido.

### Verificación final

```text
cd compiler
cargo test
22 passed; 0 failed
```

También se ejecutaron directamente los dos nuevos ejemplos: el positivo
imprimió `2`, `[1, 3, 4]`, `[1, 3, 4]`, `3`; el negativo falló con `E1053`.

### Siguiente paso recomendado en esa sesión

Continuar con la verificación de exhaustividad de `match` en el type-checker,
que ahora es la primera tarea de la sección 8. Al implementarla, añadir un
ejemplo positivo exhaustivo y otro negativo, actualizar la tabla de errores si
hace falta, y volver a ejecutar las 22 pruebas completas. Esta tarea quedó
resuelta en la bitácora siguiente.

---

## 11. Bitácora de continuación — 2026-09-17 (match y logo)

### Exhaustividad de `match`

- Añadí al `Checker` el registro de variantes de enums definidos por el usuario
  y de los enums núcleo `Option`, `Result` y `Ordering`.
- Añadí `Ty::Named` para poder relacionar parámetros/retornos como `Shape` o
  `TrafficLight` con su conjunto de variantes.
- El checker ahora valida `match` dentro de funciones y métodos de `impl`.
  Para métodos, inyecta el tipo del receptor `self` a partir del tipo del
  `impl`, por lo que `impl Shape` también se verifica.
- `E1060` enumera las variantes faltantes. `_` y un binding simple cubren el
  resto; un brazo con `guard` no cuenta como cobertura total.
- Corregí el intérprete: un patrón identificador que nombra una variante
  conocida (`Red`, `Point`, etc.) solo coincide con esa variante. Antes, tras
  fallar la primera coincidencia, se convertía accidentalmente en un binding y
  capturaba cualquier otra variante.
- La cobertura de campos anidados ya tiene la función base en el checker, pero
  la validación completa de forma/tipos de patrones y la sintaxis anidada
  posicional del parser siguen pendientes.

### Ejemplos y pruebas añadidos

- `examples/match_exhaustive.ostrin`: enum de tres variantes, match completo y
  ejecución `red/yellow/green`.
- `examples/match_non_exhaustive.ostrin`: falta `Green`; además prueba que
  `Green if true` no cuenta como cobertura exhaustiva.
- Añadí dos pruebas de integración; la suite pasó de 22 a 24 pruebas.

### Logo

- Revisé el archivo externo `C:\Users\Andre\Downloads\gemini-svg.svg`.
  Visualmente combina corchetes azules (genéricos/sintaxis) con una marca de
  calibración ámbar (cantidades físicas/unidades), así que encaja con la
  identidad técnica de Ostrin.
- Copié la propuesta sin cambios geométricos a
  `assets/ostrin-logo.svg`, cambiando únicamente los comentarios a minúsculas
  para que quede como asset oficial del proyecto.
- No se convirtió a PNG ni se generó una variante adicional: el SVG es limpio,
  pequeño y escalable; las variantes claro/oscuro pueden decidirse más adelante.

### Verificación

```text
cd compiler
cargo test
24 passed; 0 failed
```

La prueba positiva imprime `red`, `yellow`, `green`; el caso negativo produce
`OSTRIN-E1060` con `Green` como variante faltante.

### Siguiente paso recomendado

La verificación real de generics `<T: Trait>`, la validación básica de forma y
tipos de patrones y la instanciación de enums/records genéricos quedaron
implementadas en las bitácoras siguientes. El próximo paso recomendado es
completar la coherencia de traits y la herencia avanzada: regla orphan,
colisiones entre supertraits, defaults transitivos y argumentos genéricos
explícitos.

---

## 12. Bitácora de continuación — 2026-09-17 (generics y trait bounds)

### Alcance implementado

- `Ty` ahora distingue `Generic("T")` de un tipo nominal concreto. Esto evita
  que un parámetro genérico se trate como `Unknown` y permite comprobar su uso
  dentro de la función que lo declara.
- `FnSig` conserva los parámetros genéricos de cada función. El `Checker`
  registra además las declaraciones `trait` y las parejas nominales
  `impl Trait for Type` antes de verificar cuerpos y llamadas.
- Durante el chequeo de una función, sus bounds activos se cargan en el scope
  semántico. Un método como `a.equals(b)` dentro de `fn same<T: Comparable>`
  solo se acepta si `Comparable` declara ese método; el retorno y los
  parámetros `Self` se resuelven contra `T`.
- En una llamada genérica se unifican parámetros simples y contenedores
  (`List<T>`, `Map<K,V>`, `Set<T>`), se detectan inferencias incompatibles y se
  comprueba que cada tipo concreto tenga la implementación nominal de todos
  sus bounds. Los fallos usan `E1042`, añadido al registro canónico del
  documento 17.
- Se mantuvo separada la lógica previa de dimensiones: `<D: Dimension>` sigue
  resolviendo `Quantity<D>` mediante sustitución de dimensiones y no se mezcla
  con los generics nominales normales.
- Al hacer que los literales de `record` produzcan su tipo nominal, aparecieron
  dos regresiones que también quedaron corregidas: `Self` en retornos de
  métodos `impl`, y operaciones como `Vector2 + Vector2` cuando existe
  `impl Add for Vector2`. La compatibilidad de tipos ahora recorre
  contenedores, lo que conserva el caso `List<dyn Shape>`.

### Ejemplos y pruebas añadidos

- `examples/generics.ostrin`: declara `Comparable`, implementa el trait para
  `Score`, verifica `same<T: Comparable>` y demuestra inferencia de `T` con
  `first<T>(List<T>) -> T`.
- `examples/generics_bound_error.ostrin`: intenta pasar `Plain` a
  `same<T: Comparable>` sin `impl Comparable`; falla de forma intencional con
  `E1042`.
- Añadí dos pruebas de integración; la suite pasó de 24 a 26 pruebas.

### Verificación final

```text
cd compiler
cargo test
26 passed; 0 failed
```

El ejemplo positivo imprime `true`, `false`, `7`. El negativo menciona
`E1042`, `Plain` y `Comparable` en el diagnóstico.

### Límites conocidos de esta iteración

La implementación no pretende cerrar todavía argumentos genéricos explícitos
ni la interacción completa entre generics, supertraits y defaults transitivos.
Estas fronteras quedan anotadas para no confundir esta validación básica con un
sistema de traits/generics de producción.

---

## 13. Bitácora de continuación — 2026-09-17 (patrones anidados)

### Alcance implementado

- El parser ahora acepta constructores anidados, como
  `Wrapped(Ready(value))`, además de patrones con campos explícitos como
  `Point(x: x, y: _)`.
- Los campos posicionales se resuelven por índice y los campos nombrados por
  nombre. La forma corta (`Circle(radius)`) conserva el azúcar documentado y
  se valida contra la declaración del constructor.
- El checker registra nombres y tipos de campos de variantes y records. Cada
  brazo de `match` valida que el constructor pertenezca al tipo escrutado, que
  la aridad sea correcta, que los campos existan y que los literales/rangos
  sean compatibles con el tipo esperado.
- Los bindings introducidos por patrones anidados reciben el tipo del campo,
  en lugar de entrar al scope como `Unknown`. Un constructor con datos no puede
  usarse desnudo (`Red => ...`); debe desestructurarse (`Red(value)`) o cubrirse
  con `_`.
- El intérprete resuelve correctamente campos posicionales y nombrados en
  enums y records, incluyendo un constructor anidado dentro de otro.
- `E1061` se añadió al registro canónico del documento 17 para los errores de
  forma, campo o tipo de patrón.

### Ejemplos y pruebas añadidos

- `examples/match_nested.ostrin`: prueba un enum anidado con campos
  posicionales (`Wrapped(Ready(value))`) y un record con campos nombrados
  (`Point(x: x, y: _)`).
- `examples/match_pattern_error.ostrin`: prueba campo desconocido, literal con
  tipo incorrecto y uso desnudo de un constructor con datos.
- Añadí dos pruebas de integración; la suite pasó de 28 a 30 pruebas.

### Verificación final

```text
cd compiler
cargo test
30 passed; 0 failed
```

El ejemplo positivo imprime `42`, `0`, `5`. El negativo falla con `E1061` y
reporta el campo `other`, el literal `Bool` incompatible y el constructor con
datos.

### Límites conocidos de esta iteración

La cobertura exhaustiva sigue siendo conservadora para estructuras recursivas
que superan la profundidad de análisis acotada. La sintaxis de argumentos
genéricos explícitos y la semántica completa de traits quedan como siguientes
pendientes explícitos.

---

## 14. Bitácora de continuación — 2026-09-17 (instanciación genérica y cobertura)

### Alcance implementado

- `Ty` ahora distingue un tipo aplicado (`Applied("Maybe", [Int])`) de un
  nombre nominal sin argumentos. `resolve_type` conserva la representación
  dimensional especial de `Quantity<D>` y aplica recursivamente argumentos en
  tipos de usuario.
- El checker registra los parámetros genéricos de `enum` y `record`. Los
  constructores de variantes infieren sus argumentos desde los valores (`Just(9)`
  produce `Maybe<Int>`), y los literales de records hacen lo mismo
  (`Pair { first: 4, second: 8 }` produce `Pair<Int>`).
- Las declaraciones de funciones genéricas pueden recibir y devolver esos
  tipos aplicados. La unificación existente de `T` se extendió a nombres de
  usuario aplicados y permite que `Nothing` introduzca argumentos `Unknown` que
  luego se resuelven con otro argumento de la llamada.
- Los patrones consultan la instanciación real del enum/record: `Just(item)`
  liga `item` al `T` concreto, y `Just(Good(number))` valida ambos niveles.
- La exhaustividad enumera formas finitas de constructores anidados y exige
  cubrir todas las combinaciones conocidas. Por ejemplo, `Just(Good(_))` sin
  `Just(Bad(_))` se reporta como cobertura incompleta de `Just`, aunque la
  variante exterior sí aparezca.

### Ejemplos y pruebas añadidos

- `examples/generic_nested_patterns.ostrin`: prueba `Maybe<T>`,
  `Outcome<T,E>`, `Pair<T>`, inferencia desde constructores y patterns
  anidados completos.
- `examples/generic_nested_patterns_error.ostrin`: omite la combinación
  `Just(Bad(...))` y falla con `E1060` mencionando `Just`.
- Añadí dos pruebas de integración; la suite pasó de 30 a 32 pruebas.

### Verificación final

```text
cd compiler
cargo test
32 passed; 0 failed
```

El ejemplo positivo imprime `9`, `7`, `good`, `failed`, `empty`, `4`. El caso
negativo demuestra la detección de cobertura incompleta en el interior de un
constructor genérico.

### Límites conocidos de esta iteración

La enumeración de formas limita la profundidad y el total de formas para evitar
ciclos o explosiones combinatorias en tipos recursivos como `Tree<T>`. Todavía
faltan argumentos genéricos explícitos y la interacción avanzada entre
generics, supertraits y defaults.

---

## 15. Bitácora de continuación — 2026-09-17 (semántica básica de traits)

### Alcance implementado

- `Param` conserva `is_mut`, de modo que el AST distingue `self` de `mut self`.
  Esto mantiene disponible la comprobación de mutabilidad del receptor y
  permite comparar esa parte de la firma entre un trait y su `impl`.
- El type-checker valida supertraits directos: `impl Child for Value` exige que
  también exista `impl Parent for Value` cuando `trait Child: Parent` (`E1050`).
- Un `impl Trait for Type` debe proporcionar todos los métodos del trait que no
  tienen cuerpo default (`E1055`). Los métodos extra siguen permitidos, para no
  romper el despacho existente por nombre.
- Las firmas de los métodos declarados en el trait y en el `impl` se comparan
  en nombre, mutabilidad del receptor, parámetros, tipos, retorno y generics.
  Las incompatibilidades, los `impl` duplicados y los traits no declarados (con
  la excepción de los traits estándar ya usados por el prototipo) producen
  `E1054`.
- El intérprete registra las declaraciones de traits y, al buscar un método,
  usa el método explícito del `impl` si existe; si no, materializa el cuerpo
  default del trait. Esto hace ejecutables los defaults básicos sin cambiar el
  despacho de los `impl` existentes.
- Los traits estándar (`Add`, `Sub`, `Mul`, `Div`, `Eq`, `Ord`, `Iterator`,
  `Printable`, `Default`, `Hash`, `Drop`) se aceptan aunque no estén declarados
  en el archivo, porque los ejemplos actuales los usan como protocolo
  incorporado del prototipo.

### Ejemplos y pruebas añadidos

- `examples/traits_defaults.ostrin`: prueba un supertrait satisfecho y un
  método default heredado en runtime.
- `examples/traits_semantics_errors.ostrin`: concentra los fallos de método
  requerido ausente (`E1055`), supertrait faltante (`E1050`) y firma
  incompatible (`E1054`).
- Añadí dos pruebas de integración; la suite pasó de 30 a 32 pruebas.

### Verificación final

```text
cd compiler
cargo test
32 passed; 0 failed
```

El ejemplo positivo imprime `7`. El ejemplo negativo falla y contiene los
códigos `E1050`, `E1054` y `E1055`.

### Límites conocidos de esta iteración

La semántica implementada es intencionalmente básica: faltan la regla orphan,
la detección completa de colisiones entre supertraits, herencia/defaults
transitivos, argumentos genéricos explícitos y el chequeo semántico completo de
los cuerpos default. La comparación de firmas cubre los casos directos del
prototipo, pero todavía no modela todas las sustituciones genéricas posibles.

### Siguiente paso recomendado

Implementar coherencia de traits (orphan rule y colisiones), después extender
la herencia/defaults a múltiples niveles y cerrar la sintaxis de argumentos
genéricos explícitos. Cada extensión debe añadir primero un `.ostrin` positivo
y otro negativo a `compiler/tests/examples.rs`.

---

## 16. Bitácora de continuación — 2026-09-17 (coherencia de traits)

### Alcance implementado

- El AST conserva el módulo de origen de cada `record`, `enum`, `trait` e
  `impl`. El linker de módulos ahora reescribe también nombres de traits en
  `impl`, supertraits y imports explícitos de traits públicos.
- La regla `orphan` se aplica después del aplanado de módulos: un `impl Trait
  for Type` solo es válido si el trait o el tipo fue definido en el módulo que
  contiene el `impl`. El caso trait externo + tipo externo falla con `E1056`;
  un trait local sobre un tipo externo sigue permitido.
- La comprobación de supertraits dejó de ser solo directa. La clausura
  transitiva exige todas las implementaciones ancestrales (`Child: Middle`,
  `Middle: Root` exige `Middle` y `Root`), usando `E1050` cuando falta alguna.
- Se validan ciclos y declaraciones incompatibles de métodos heredados. Dos
  supertraits pueden compartir un método si su firma es idéntica; si difieren,
  el trait compuesto falla con `E1057`. También se detectan supertraits o
  métodos declarados repetidos.
- `E1054` conserva los casos de trait desconocido, `impl` duplicado y firma
  incompatible; `E1056` y `E1057` quedaron añadidos al registro canónico del
  documento 17.

### Ejemplos y pruebas añadidos

- `examples/traits_coherence_errors.ostrin`: herencia transitiva incompleta y
  colisión incompatible entre `Left` y `Right`.
- `examples/proj_orphan/`: prueba `impl` externo contra trait y tipo externos;
  falla con `E1056`.
- `examples/proj_coherence_allowed/`: confirma el caso permitido de trait
  local implementado para un tipo definido en otro módulo.
- Añadí tres pruebas de integración; la suite pasó de 32 a 35 pruebas.

### Verificación final

```text
cd compiler
cargo test
35 passed; 0 failed
```

La prueba de coherencia reporta `E1050` y `E1057`; las dos pruebas de módulos
confirman rechazo `E1056` y aceptación del caso local, respectivamente.

### Límites conocidos de esta iteración

La regla todavía opera sobre nombres de tipos simples porque el AST de `impl`
descarta los argumentos genéricos después de parsearlos. Siguen pendientes la
sintaxis de argumentos genéricos explícitos, la coherencia completa para tipos
aplicados, la herencia/defaults transitivos en el despacho del intérprete y la
validación semántica de cuerpos default.

### Siguiente paso recomendado

Conservar los argumentos genéricos del `impl` en el AST y añadir la sintaxis de
argumentos genéricos explícitos en llamadas y constructores. Después se puede
extender la resolución runtime de defaults a la clausura de supertraits sin
perder la validación de coherencia ya cubierta.

---

## 17. Bitácora de continuación — 2026-09-17 (argumentos genéricos explícitos)

### Alcance implementado

- `Expr` incorpora `GenericCall`, separado de las llamadas ordinarias, para
  conservar expresiones como `identity<Int>(7)` hasta el type-checker y el
  intérprete.
- El parser reconoce una lista de tipos entre `<...>` antes de los argumentos
  de una llamada y conserva la sintaxis existente de comparaciones cuando no
  aparece una llamada después del `>`.
- Las llamadas a funciones genéricas aceptan argumentos explícitos completos.
  El checker valida aridad, unifica esos tipos con los tipos de los valores,
  comprueba los bounds nominales y resuelve el tipo de retorno (`E1042`). Los
  parámetros dimensionales explícitos también alimentan la sustitución de
  `Quantity<D>`.
- El intérprete trata los argumentos genéricos como información estática y
  ejecuta la misma función monomorfizada por el prototipo; no se duplicó la
  lógica de runtime.
- Los argumentos explícitos en constructores, métodos genéricos y declaraciones
  `impl` todavía producen un diagnóstico claro de alcance no soportado, en vez
  de caer silenciosamente a inferencia.

### Ejemplos y pruebas añadidos

- `examples/generics_explicit.ostrin`: selecciona `Int` y `String` de forma
  explícita en llamadas a funciones genéricas y conserva la ejecución real.
- `examples/generics_explicit_errors.ostrin`: prueba aridad incorrecta, tipo
  incompatible y bound `Comparable` ausente.
- Añadí dos pruebas de integración; la suite pasó de 35 a 37 pruebas.

### Verificación final

```text
cd compiler
cargo test
37 passed; 0 failed
```

El ejemplo positivo imprime `7` y `a`; el negativo contiene `E1042` y reporta
la aridad, el tipo incompatible y `Plain` sin el bound requerido.

### Límites conocidos de esta iteración

La detección de llamadas genéricas sigue teniendo la ambigüedad sintáctica
natural entre `<...>` y comparaciones cuando una expresión adopta la forma
`a < T > (x)`. Además, no se conservan todavía argumentos genéricos en
`ImplDecl`, los constructores no aceptan tipos explícitos y los métodos de
traits no tienen un nodo de llamada genérica independiente.

### Siguiente paso recomendado

Extender `ImplDecl` para conservar parámetros y argumentos de tipo, luego
añadir `GenericCall` a métodos y constructores con una prueba de inferencia y
otra de error por aridad/bound. Finalmente, revisar la ambigüedad de `<...>`
con una regla léxica o una restricción sintáctica explícita.

---

## 18. Bitácora de continuación — 2026-09-17 (generics en constructores, métodos e impls)

### Alcance implementado

- `ImplDecl` ya no descarta información genérica: conserva sus parámetros, los
  argumentos del trait y los argumentos del tipo implementado. El checker
  valida la aridad contra traits, records, enums y tipos incorporados
  (`Quantity`, `List`, `Map`, `Set`) y pone los generics del `impl` en el scope
  al revisar sus cuerpos.
- Los constructores de variantes aceptan llamadas como `Just<String>(value)`.
  Los records genéricos aceptan la forma `Box<Int> { value: 1 }`; ambas formas
  comprueban aridad, sustitución de campos y compatibilidad de valores.
- Los métodos genéricos accesibles a través de un trait bound aceptan la forma
  `container.map<U>(value)`, con inferencia cuando se omite `<U>`, validación de
  bounds y sustitución de `Self`/parámetros en el retorno.
- El intérprete conserva una sola representación runtime: los argumentos de
  tipo se validan estáticamente y luego se ejecuta la función o método existente
  sin generar copias del valor.
- La gramática consolidada del documento 17 incluye llamadas genéricas y
  records genéricos literales.

### Ejemplos y pruebas añadidos

- `examples/generic_impls_and_methods.ostrin`: `impl<T> Mapper for Box<T>`,
  método genérico `map<U>`, constructor `Box<Int>` y llamada
  `apply<Box<Int>, String>(...)`.
- Se extendió `examples/generics_explicit.ostrin` para cubrir un constructor
  de enum (`Just<String>`), un parámetro dimensional explícito
  (`keep_unit<Length>`) y un `impl<D: Dimension>` sobre `Quantity<D>`.
- Añadí una prueba de integración; la suite pasó de 37 a 38 pruebas.

### Verificación final

```text
cd compiler
cargo test
38 passed; 0 failed
```

El ejemplo de generics e impls imprime `ok`; el de argumentos explícitos
imprime `7`, `a`, `Just(a)`, `5 m` y `1`.

### Límites conocidos de esta iteración

Los `impl` genéricos todavía comparten despacho por nombre de tipo y no forman
instancias especializadas por argumentos; falta sustituir completamente los
argumentos del trait/tipo dentro de firmas heredadas y comprobar bounds de
todos los parámetros del `impl`. Los métodos genéricos se validan de forma
completa cuando el receptor llega mediante un trait bound; los métodos de
tipos concretos aún dependen del chequeo runtime existente.

### Siguiente paso recomendado

Modelar una clave de despacho con tipo aplicado (`Box<Int>`, no solo `Box`) y
cerrar la sustitución de argumentos de trait en firmas y defaults. Después se
puede mejorar la ambigüedad sintáctica entre llamadas genéricas y el operador
`<` sin cambiar la información que ya conserva el AST.

---

## 19. Bitácora de continuación — 2026-09-17 (traits genéricos y defaults transitivos)

### Alcance implementado en esta continuación

- Se cerró la sustitución de argumentos de traits dentro de las firmas de sus
  métodos. Por ejemplo, en `impl Convert<Int> for Value`, la firma heredada de
  `trait Convert<T>` se compara como `convert(self, Int) -> Int`, no como una
  firma que todavía contenga el símbolo genérico `T`.
- Se añadió `specialize_trait_method` y un reescritor recursivo de `Type` para
  sustituir parámetros del trait en parámetros, retornos, contenedores, tipos
  funcionales y expresiones dimensionales anidadas sin modificar el AST
  original del trait.
- La validación de bounds de métodos genéricos ahora se aplica tanto cuando el
  tipo se infiere como cuando se proporciona explícitamente (`method<U>(...)`).
  También se actualizó un comentario obsoleto del intérprete que decía que los
  defaults todavía no se heredaban.
- El despacho runtime de defaults ya fue probado en una cadena de tres niveles:
  `Leaf` usa el default de `Middle`, que usa el default de `Root`. El modelo
  actual exige que existan los `impl` de los traits ancestrales, de acuerdo con
  la validación de supertraits transitivos.

### Ejemplos y pruebas añadidos

- `examples/generic_trait_args_errors.ostrin`: incluye un `impl Convert<Int>`
  válido y otro incompatible (`String`), que falla con `E1054`.
- `examples/traits_defaults_transitive.ostrin`: ejecuta la cadena de defaults y
  produce `9`.
- Añadí dos pruebas de integración; la suite pasó de 38 a 40 pruebas.

### Verificación final

```text
cd compiler
cargo test
40 passed; 0 failed
```

### Límites actuales y siguiente paso

Los argumentos genéricos ya se conservan, se reescriben al importar módulos y
se usan para validar aridad y firmas. El intérprete todavía despacha por el
nombre base del tipo (`Box`) y no por una clave aplicada (`Box<Int>`), por lo
que aún no existen instancias runtime separadas para distintos argumentos.
Tampoco se validan todavía de forma completa todos los bounds de parámetros de
`impl` ni los cuerpos `default` con el contexto especializado del trait.

El siguiente paso recomendado es introducir una representación de tipo aplicado
en la selección runtime de impls, acompañada de un ejemplo con dos instancias
del mismo tipo genérico que requieran comportamientos distintos. Después debe
cerrarse la validación de bounds y cuerpos especializados antes de considerar
el sistema de generics listo para un backend de compilación real.

---

## 20. Bitácora de continuación — 2026-09-17 (despacho runtime por tipo aplicado)

### Alcance implementado en esta continuación

- El intérprete ahora conserva cada `impl` como una unidad con sus generics,
  `trait_name`, argumentos del tipo implementado y métodos. Antes agrupaba solo
  los métodos por el nombre base (`Box`), lo que podía seleccionar el impl
  equivocado cuando había varias aplicaciones del mismo record.
- Los records conservan sus argumentos aplicados en una tabla asociada a su
  storage de `Rc`. `Box<Int>` y `Box<String>` llegan así al selector runtime con
  información distinta; los records sin sintaxis explícita intentan inferir sus
  argumentos desde los tipos de sus campos.
- La coincidencia de impls soporta argumentos concretos, variables genéricas
  (`Box<T>`), variables repetidas y tipos anidados. Si el runtime no puede
  materializar argumentos de un valor incorporado, solo permite el fallback de
  un impl cuyos argumentos sean completamente genéricos; un impl concreto no se
  aplica silenciosamente.
- El fallback de métodos `default` usa la misma selección aplicada, por lo que
  no se separó el comportamiento de métodos explícitos y defaults.

### Ejemplos y pruebas añadidos

- `examples/generic_impl_dispatch.ostrin`: `Box<Int>` y `Box<String>` usan
  traits distintos con el mismo método `label` y producen `integer` y `text`.
- `examples/generic_impl_dispatch_error.ostrin`: intenta usar el impl de
  `Box<Int>` sobre `Box<String>` y falla con `no method 'label'`.
- Añadí dos pruebas de integración; la suite pasó de 40 a 42 pruebas.

### Verificación final

```text
cd compiler
cargo test
42 passed; 0 failed
```

### Límites actuales y siguiente paso

La selección aplicada ya funciona para records genéricos y mantiene el
comportamiento existente de `Quantity<D>` mediante el fallback genérico. Los
enums genéricos y aplicaciones concretas de tipos incorporados todavía no
conservan el mismo metadato runtime; además, las llamadas a métodos concretos
siguen sin resolución estática completa y pueden producir un error al ejecutar.

El siguiente paso recomendado es extender el metadato a enums y cantidades
cuando se necesite despachar impls concretos para ellos, y después mover la
resolución de métodos concretos al type-checker para que los casos inválidos
fallen antes de ejecutar.

---

## 21. Bitácora de continuación — 2026-09-17 (aplicaciones en enums y Quantity)

### Alcance implementado en esta continuación

- `Value::EnumInstance` ahora conserva sus argumentos genéricos aplicados. Los
  constructores explícitos (`Item<Int>(1)`) los guardan directamente y los
  constructores sin argumentos explícitos intentan inferirlos desde los campos
  de la variante.
- `Quantity<D>` expone al selector runtime una aplicación estable basada en su
  dimensión. Las dimensiones base se representan como `Length`, `Time`,
  `Mass`, etc., de modo que `Quantity<Length>` y `Quantity<Time>` pueden tener
  impls diferentes sin mezclarse.
- La coincidencia de impls aplicada se reutiliza para records, enums y
  quantities. Se mantuvo el fallback limitado para impls completamente
  genéricos cuando una representación runtime no puede materializar todos sus
  argumentos.
- Se actualizaron todos los constructores internos de `Option`, `Result` y
  `Ordering`, además de los patrones de `match`, comparación y ordenamiento,
  para transportar la nueva metadata sin alterar su formato visible.

### Ejemplos y pruebas añadidos

- `examples/generic_enum_dispatch.ostrin`: `Item<Int>` y `Item<String>` usan
  impls diferentes con el mismo método `label`.
- `examples/quantity_impl_dispatch.ostrin`: `Quantity<Length>` y
  `Quantity<Time>` seleccionan impls diferentes.
- Añadí dos pruebas de integración; la suite pasó de 42 a 44 pruebas.

### Verificación final

```text
cd compiler
cargo test
44 passed; 0 failed
```

### Límites actuales y siguiente paso

El despacho aplicado ya cubre records, enums y dimensiones base de
`Quantity`. Las dimensiones compuestas todavía se representan de forma
conservadora para este selector, y la resolución de métodos sobre tipos
concretos sigue siendo principalmente runtime: el checker sí valida métodos
accesibles mediante trait bounds, pero no rechaza todavía todos los métodos
concretos inexistentes antes de ejecutar.

El siguiente paso recomendado es llevar esa resolución al type-checker,
reutilizando la misma coincidencia de argumentos aplicados, para producir un
diagnóstico estático en vez de `no method ...` durante la ejecución. Después se
pueden completar bounds y cuerpos `default` con sustitución especializada.

---

## 22. Bitácora de continuación — 2026-09-17 (resolución estática de métodos concretos)

### Punto de partida

La continuación se había interrumpido justo después de empezar a mover el
despacho de métodos concretos al type-checker. El estado intermedio rechazaba
incorrectamente `Box<Int>`, `Box<String>` y `Boxed<...>` porque el matcher
comparaba `Type::Named("Int")` con `Ty::Int` como si fueran representaciones
distintas. También trataba `Quantity.to_string()` como un método de usuario,
aunque `to_string` es una operación incorporada del runtime.

### Cambios realizados

- Se completó la selección estática de métodos concretos mediante una
  candidatura que reutiliza la coincidencia de tipos aplicados para records,
  enums y `Quantity`.
- Las aplicaciones concretas de tipos básicos (`Int`, `String`, etc.) ahora se
  comparan contra sus variantes `Ty` correctas; se conservan además las
  sustituciones de parámetros genéricos del `impl`.
- El checker valida métodos concretos con sus argumentos explícitos o
  inferidos, bounds genéricos, cantidad y tipos de argumentos, y tipo de
  retorno. Los argumentos genéricos del trait se especializan antes de validar
  la llamada.
- `to_string` se reconoce como operación incorporada para tipos concretos,
  manteniendo la compatibilidad con `shapes.ostrin`.
- `examples/concrete_method_errors.ostrin` y su prueba de integración cubren
  el rechazo estático de un argumento inválido.
- El caso que antes esperaba `no method ...` en runtime ahora comprueba el
  diagnóstico estático `E1042` para `Box<String>`.

### Verificación final

```text
cd compiler
cargo test
45 passed; 0 failed
```

### Límites actuales y siguiente paso

La resolución estática cubre las llamadas de métodos concretos que el checker
puede representar; `spawn`, `channel` y algunos combinadores dinámicos siguen
conservando resultados `Unknown` por el alcance deliberado del prototipo. Los
cuerpos `default` aún no se validan con un contexto especializado completo y
los bounds de todos los parámetros de `impl` siguen siendo una tarea separada.
El siguiente paso recomendado es cerrar esa sustitución especializada en
cuerpos `default` y métodos de `impl`, incluyendo dimensiones compuestas.

---

## 23. Bitácora de continuación — 2026-09-17 (defaults y bounds de `impl`)

### Alcance implementado

- Los cuerpos `default` de los traits ahora se convierten en funciones
  temporales para el checker y se validan con los genéricos del trait y del
  método. `Self` recibe como bounds el trait y todos sus supertraits, así que
  llamadas como `self.base()` y cadenas transitivas de defaults se comprueban
  dentro del contexto nominal correcto.
- Los bounds declarados en parámetros genéricos de `impl` se validan contra
  traits conocidos. Además, la selección de un impl aplicado comprueba esos
  bounds con los argumentos reales; por ello un `impl<T: Comparable> ... for
  Box<T>` no se aplica a `Box<Plain>` si `Plain` no implementa `Comparable`.
- Los métodos de un `impl` se verifican con el tipo completo del receptor
  (`Box<T>`, `Quantity<D>`, etc.) en lugar del nombre base solamente. Esto
  permite resolver llamadas internas sobre `self` dentro de impls genéricos.
- Se añadió reconocimiento de dimensiones base y compuestas para evaluar el
  bound especial `Dimension` sin confundirlo con un tipo de usuario.

### Ejemplos y pruebas añadidos

- `examples/trait_default_errors.ostrin`: cuerpo `default` con retorno
  incompatible.
- `examples/impl_bounds_errors.ostrin`: impl aplicado que no cumple el bound
  de su parámetro genérico.
- La suite pasó de 45 a 47 pruebas.

### Verificación final

```text
cd compiler
cargo test
47 passed; 0 failed
```

### Límites actuales y siguiente paso

La validación de defaults ya cubre firmas y cuerpos genéricos, pero todavía no
materializa una instancia separada del cuerpo para cada combinación concreta
de argumentos del trait; los operadores sobre genéricos con bounds también
conservan el alcance acotado del checker actual. El siguiente paso es cerrar
esa especialización completa, especialmente para dimensiones compuestas y
operadores definidos por traits.

---

## 24. Dirección del producto y convención oficial — 2026-09-17

Ostrin no se limitará a cálculos: el objetivo es evolucionar desde el
intérprete/prototipo actual hacia un lenguaje utilizable para ejecutables,
aplicaciones de escritorio, backends web, interfaces interactivas y programas
científicos o de ingeniería.

### Convención de nombres

- La extensión oficial de código fuente es **`.ostrin`**.
- El identificador de lenguaje para editores y LSP será **`ostrin`**.
- El compilador seguirá llamándose **`ostrinc`** (`ostrinc.exe` en Windows).
- El manifiesto de paquetes será **`ostrin.toml`** y el lockfile
  **`ostrin.lock`**.
- No se adoptan `.ost` ni `.os`: `.ost` es ambiguo y puede confundirse con
  archivos de Outlook; `.os` es demasiado corto y genérico.

### Ruta de producto acordada

1. Cerrar el sistema de tipos y reducir los caminos `Unknown`.
2. Completar `Option`/`Result`, acceso a campos, argumentos por nombre/defaults,
   operadores por traits y especialización genérica.
3. Añadir diagnósticos con archivo, línea y columna y una CLI estable.
4. Crear la extensión de Visual Studio Code y el servidor LSP reutilizando el
   lexer, parser y checker de Ostrin.
5. Construir una biblioteca estándar mínima: consola, archivos, strings, JSON,
   fechas, procesos y red.
6. Permitir empaquetar aplicaciones como `.exe`; después añadir backend nativo.
7. Implementar runtime de recursos y concurrencia real, incluyendo cancelación
   y `select` sobre canales.
8. Añadir bindings gráficos, WebAssembly y herramientas profesionales.

La extensión `.ostrin` queda fijada desde este punto para todos los ejemplos,
módulos, paquetes y herramientas futuras.

---

## 25. Logo oficial — 2026-09-17

El usuario eligió como identidad visual de Ostrin el logo geométrico con dos
formas blancas superiores, dos formas violetas inferiores y centro oscuro,
proporcionado como imagen PNG.

- Recurso oficial actual: `assets/ostrin-logo.png`.
- El logo SVG anterior se conserva únicamente como referencia histórica en
  `assets/archive/ostrin-logo-concept.svg`.
- El PNG oficial se utilizará en la extensión de VS Code, documentación,
  paquetes y futuras aplicaciones de Ostrin.

---

## 26. Primera integración con Visual Studio Code — 2026-09-17

Se creó `vscode-ostrin/` como primera extensión funcional, sin dependencias
externas, para comenzar a convertir Ostrin en una herramienta de desarrollo
usable.

### Incluido

- Reconocimiento oficial de archivos `.ostrin`.
- Resaltado TextMate para palabras reservadas, tipos, constructores, funciones,
  operadores, strings, números y literales con unidades.
- Configuración de comentarios, brackets, autocierre y regiones plegables.
- Logo oficial en `vscode-ostrin/images/ostrin-logo.png`.
- Comando `Ostrin: Check Current File`.
- Comando `Ostrin: Run Current File`.
- Configuración `ostrin.compilerPath` para localizar `ostrinc`.
- Configuración opcional `ostrin.checkOnSave`.

### Verificación

- `package.json`, `language-configuration.json` y la gramática TextMate pasan
  validación JSON.
- `extension.js` pasa la comprobación sintáctica de Node.js.
- El logo de la extensión coincide byte a byte con `assets/ostrin-logo.png`.
- La suite del compilador permanece en 47 pruebas exitosas.

### Siguiente ampliación

La extensión todavía no implementa LSP, autocompletado ni diagnósticos con
rangos. Para eso el compilador deberá exponer ubicaciones de origen y salida
estructurada; la primera extensión deja preparada la asociación y el flujo de
comandos mientras se completa esa base semántica.

---

## 27. Sitio web multipágina y superficie pública — 2026-09-17

La web pública de Ostrin evolucionó desde una landing de una sola página hacia
un sitio estático multipágina publicado en GitHub Pages:

- `index.html`: portada editorial con propuesta de valor, código de ejemplo,
  métricas y entrada al ecosistema.
- `language.html`: principios del lenguaje, cantidades físicas, estado,
  errores, abstracciones y dirección de concurrencia.
- `examples.html`: catálogo de 68 archivos `.ostrin`, filtros por categoría y
  laboratorio visual con pestañas de ejemplos y botón de copiar.
- `ecosystem.html`: compilador, intérprete, VS Code, documentación, runtime,
  empaquetado nativo, WebAssembly y estado real de cada superficie.
- `docs.html`: quick start, comandos de `ostrinc`, mapa de los 17 documentos de
  diseño y guía inicial de VS Code.
- `roadmap.html`: línea de tiempo pública que separa trabajo completado,
  siguiente etapa y capacidades futuras.

### Decisiones de diseño

- La estética sigue una dirección editorial/técnica: tipografía grande,
  reglas finas, paneles de código, contraste oscuro/papel y uso sobrio del
  logo oficial.
- La navegación funciona con HTML estático y tiene menú responsive para
  pantallas pequeñas.
- `site.js` proporciona menú móvil, filtros de ejemplos, pestañas del
  laboratorio y copiado al portapapeles.
- Las capacidades futuras —LSP, biblioteca estándar, ejecutables, backend
  nativo, WebAssembly y registro de paquetes— aparecen como roadmap y no como
  funcionalidades ya disponibles.

### Verificación

- Las páginas locales cargan correctamente: portada, lenguaje, ejemplos,
  ecosistema, documentación, roadmap y 404.
- No se encontraron enlaces relativos rotos en HTML ni referencias locales de
  estilos/recursos faltantes.
- `node --check website/site.js` pasa.
- `cargo test` permanece en **49 pruebas exitosas**.

---

## 28. Acceso estático a campos de records — 2026-09-17

Se cerró uno de los caminos más visibles de `Unknown` en el checker: los
accesos a campos de records.

- `p.value` ahora devuelve el tipo declarado del campo en lugar de degradar a
  `Unknown`.
- Los records genéricos sustituyen sus parámetros al acceder a un campo; por
  ejemplo, `Score<Int> { ... }.value` se verifica como `Int`.
- Las asignaciones de campos comparan el tipo del valor con el tipo declarado y
  reutilizan `E1041` para incompatibilidades.
- Las asignaciones directas respetan la mutabilidad declarada en el campo y en
  el binding receptor; `mut self` también se conserva al construir el scope de
  un método.
- Un campo inexistente en un tipo de usuario produce `E1043`.
- Se añadieron `field_access.ostrin` y `field_access_errors.ostrin`, junto con
  dos pruebas de integración.

La siguiente mejora relacionada será comprobar las llamadas a funciones y los
tipos de retorno de las colecciones, sin degradar innecesariamente a
`Unknown`.

---

## 29. Llamadas con nombre/default y resultados de colecciones — 2026-09-17

Se cerró otro hueco entre el diseño y la implementación del núcleo:

- `FnSig` conserva ahora nombres, tipos y valores por defecto de los
  parámetros.
- El checker valida cantidad, nombres, duplicados, orden de argumentos y
  compatibilidad de tipos en llamadas a funciones.
- Las llamadas genéricas siguen unificando sus argumentos ya asociados al
  parámetro correcto.
- El intérprete aplica argumentos nombrados y evalúa valores por defecto al
  invocar funciones de usuario.
- `List.find`, `Map.get` y `Map.remove` devuelven `Option<T>` en el checker;
  `List.map`, `filter` y `fold` conservan o propagan el tipo que pueden
  conocer estáticamente.
- Se añadieron `function_arguments.ostrin`,
  `function_argument_errors.ostrin` y `collection_types_errors.ostrin`, más
  tres pruebas de integración.

### Verificación

`cargo test --quiet` pasa con **53 pruebas exitosas**. La comprobación de
formato global ya tenía diferencias preexistentes en archivos no tocados; no
se aplicó un formateo masivo para evitar mezclar cambios ajenos a esta etapa.

---

## 30. Tipos y argumentos de colecciones — 2026-09-17

La validación estática de las colecciones dejó de limitarse al tipo de retorno:

- `List`, `Map` y `Set` comprueban la cantidad de argumentos de sus métodos.
- `push`, `remove_at`, `get`, `set`, `contains`, `add`, `remove` y métodos
  relacionados comparan elementos, claves, valores e índices con sus tipos
  declarados.
- Las funciones de orden superior reciben una forma estática mínima (`fn` con
  la aridad esperada) sin perder la inferencia del tipo de retorno disponible.
- Se añadió `collection_argument_errors.ostrin` y una prueba de integración.

La suite queda en **54 pruebas exitosas**. El siguiente bloque prioritario del
checker será validar contextos booleanos, retornos explícitos y expresiones de
control que todavía pueden degradar a `Unknown`.

---

## 31. Contextos booleanos y retornos explícitos — 2026-09-17

El checker ahora usa el contexto semántico de control y de retorno:

- `if` y `while` exigen una condición compatible con `Bool`.
- `and` y `or` rechazan operandos que no sean booleanos.
- `return expr` compara la expresión con el tipo de retorno declarado.
- `return` vacío solo es válido en funciones que devuelven `Void`.
- Los valores por defecto de los parámetros se comprueban contra el tipo de
  la firma.
- Las lambdas suspenden temporalmente el contexto de retorno de la función que
  las contiene para que sus retornos se comprueben en su propio bloque.
- Se añadió `control_type_errors.ostrin` y una prueba de integración.

La suite pasa con **55 pruebas exitosas**.

---

## 32. `Option<T>` y `Result<T, E>` utilizables — 2026-09-17

El modelo de resultados explícitos dejó de ser solo una representación interna:

- `Option` soporta `is_some`, `is_none`, `unwrap`, `unwrap_or`, `ok_or`,
  `map` y `then` en el checker y el intérprete.
- `Result` soporta `is_ok`, `is_err`, `unwrap`, `unwrap_or`, `ok`, `map`,
  `map_err` y `then`.
- Las operaciones conservan los parámetros genéricos en sus tipos de retorno,
  por ejemplo `Map.get`/`List.find` producen `Option<T>` y `Option.ok_or(E)`
  produce `Result<T, E>`.
- Se validan aridad y formas mínimas de las funciones callback.
- Se añadieron `option_result.ostrin` y `option_result_errors.ostrin`, junto
  con dos pruebas de integración ejecutables.

La suite queda en **57 pruebas exitosas**.

---

## 33. Propagación explícita con `try` — 2026-09-17

`try` ahora conecta la semántica estática con el runtime:

- `try Some(value)` desempaqueta el valor y `try None` retorna `None` de la
  función contenedora.
- `try Ok(value)` desempaqueta el valor y `try Err(error)` retorna el error de
  la función contenedora.
- `try expr catch fn(error) { ... }` transforma explícitamente el error antes
  de propagarlo como `Err`.
- El checker exige que el contenedor exterior sea el mismo (`Option` con
  `Option`, `Result` con `Result`) y comprueba el tipo de error propagado o del
  callback `catch`.
- Se añadieron `try_result.ostrin` y `try_errors.ostrin`, además de dos pruebas
  de integración ejecutables.

La suite pasa con **59 pruebas exitosas**.

---

## 34. Biblioteca estándar mínima: archivos y parsing — 2026-09-17

Se añadió una primera superficie real de biblioteca estándar al intérprete:

- `read_file(path: String) -> Result<String, String>` lee texto UTF-8.
- `write_file(path: String, contents: String) -> Result<Void, String>`
  escribe texto y conserva los fallos como `Err`.
- `parse_int(text: String) -> Result<Int, String>` convierte entradas sin
  lanzar un error de ejecución para datos inválidos.
- `panic(message: String)` conserva un escape explícito para bugs del programa.
- El checker conoce estas firmas y rechaza argumentos incompatibles antes de
  ejecutar.
- Se añadieron `stdlib_io.ostrin` y `stdlib_io_errors.ostrin`, con dos pruebas
  de integración. La prueba elimina su archivo temporal al terminar.

La suite queda en **61 pruebas exitosas**.

---

## 35. Diagnósticos estructurados e integración inicial con VS Code — 2026-09-17

El AST ahora conserva posiciones de origen para las sentencias y el checker
las adjunta a los errores producidos mientras analiza funciones. El CLI añade
una superficie estable para herramientas:

- `--check` hace explícito el modo de verificación estática.
- `--json` emite un objeto JSON por diagnóstico (JSON Lines), con `severity`,
  `code`, `message`, `file`, `line` y `column`.
- `--help` y `--version` hacen autodocumentado el ejecutable.
- Los errores de lexer/parser de módulos también conservan archivo y posición.

La extensión de VS Code ya ejecuta `ostrinc --check --json`, limpia los
diagnósticos anteriores y muestra los nuevos en el panel Problems, además de
conservar los comandos de ejecutar y revisar al guardar. La precisión actual
es de sentencia; el siguiente refinamiento será llevar spans a expresiones y
levantar un servidor LSP completo.

Se añadieron dos pruebas de integración para el protocolo JSON y la interfaz
CLI. La suite queda en **63 pruebas exitosas**.

---

## 36. Primera capa de lenguaje en VS Code — 2026-09-17

La extensión dejó de limitarse a colorear texto y ejecutar comandos. Ahora
registra proveedores nativos de VS Code para:

- autocompletar palabras reservadas, tipos núcleo, unidades y funciones de la
  biblioteca estándar;
- mostrar documentación contextual con hover;
- construir el outline del archivo para `fn`, `record`, `enum`, `trait` e
  `impl`.

Esta capa es deliberadamente sintáctica y no pretende simular un LSP semántico.
El compilador ya expone un índice JSON de símbolos y firmas mediante
`ostrinc --symbols --json`. La extensión lo consulta en segundo plano para
completar y construir el outline con declaraciones reales del proyecto. El
siguiente paso es hacer que ese índice sea plenamente consciente de tipos desde
el checker.

---

## 37. Índice de símbolos del compilador para tooling — 2026-09-17

Se conectó la capa de editor con datos producidos por Ostrin, en vez de
mantener todas las declaraciones de usuario como una lista fija en JavaScript.
El cambio incluye:

- spans de origen para `record`, `enum`, `trait` e `impl`, además de los que ya
  tenían las funciones;
- archivo de origen conservado en las declaraciones cargadas por módulos;
- nuevo módulo `compiler/src/symbols.rs`, que construye un índice de funciones,
  records, enums, variantes, campos, traits, implementaciones y métodos;
- firmas renderizadas con genéricos, tipos compuestos y valores por defecto;
- nuevo comando `ostrinc --symbols --json archivo.ostrin`, con un objeto JSON
  por símbolo (`kind`, `name`, `detail`, `file`, `line`, `column`);
- la extensión consulta ese índice en segundo plano al abrir o guardar un
  archivo y lo usa para completar, mostrar hover y construir el outline local.

La decisión importante es mantener dos niveles claros: el índice actual es
consciente de las declaraciones y firmas del AST, mientras que el completado
dependiente del tipo de una expresión (`valor.metodo`) queda pendiente de
exponer resultados del checker. Se añadió una prueba de integración para la
salida de símbolos y la suite queda en **64 pruebas exitosas**.

---

## 38. Miembros conscientes del tipo para VS Code — 2026-09-17

Se cerró el siguiente tramo del editor: el compilador ya puede publicar no
solo declaraciones, sino también los miembros disponibles por tipo y los
bindings locales que el checker logra inferir.

### Cambios realizados

1. `compiler/src/typeck/mod.rs`
   - Se añadió `EditorBinding`, con nombre, tipo renderizado, función, span y
     archivo de origen.
   - `Checker::check_program_with_bindings` conserva los parámetros, bindings
     declarados o inferidos, bindings creados por asignación y variables de
     iteración de `for`.
   - `check_program` mantiene su API anterior y devuelve únicamente errores,
     para no romper consumidores existentes.

2. `compiler/src/symbols.rs` y `compiler/src/main.rs`
   - Se añadió `MemberSymbol` y `collect_members`.
   - El índice incluye campos de `record`, variantes de `enum`, métodos de
     `trait`/`impl` y la superficie estable de `List`, `Map`, `Set`, `Option`,
     `Result`, `Task` y `Channel`.
   - Nuevo comando `ostrinc --members --json archivo.ostrin`. Publica JSON
     Lines con objetos `member` (`owner`, `memberKind`, `name`, `detail`) y
     `binding` (`name`, `type`, `function`, `file`, `line`, `column`).

3. `vscode-ostrin/extension.js` y `language-features.js`
   - La extensión consulta `ostrinc --members --json` en segundo plano.
   - Al escribir `receiver.` resuelve el tipo base del binding más reciente
     anterior a la posición y ofrece únicamente sus miembros.
   - Hover reconoce métodos/campos de ese tipo y también muestra el tipo de
     un binding local.
   - Se conserva el completado global de keywords, tipos, unidades y
     funciones para posiciones que no son acceso a miembros.

4. `compiler/tests/examples.rs`, `README.md` y `website/`
   - Se añadió una prueba de integración que verifica `List.push`, `Map.get`
     y el binding `numbers: List<Int>`.
   - La suite queda en **65 pruebas exitosas**.
   - La documentación y la página pública reflejan `--members` y el estado
     actual del tooling.

### Decisión y límite actual

La resolución del editor es deliberadamente pragmática: usa el binding local
más reciente cuyo span precede al cursor y extrae el nombre base de tipos como
`List<Int>` o `Option<String>`. Eso hace útil el completado inmediatamente sin
convertir todavía el compilador en un LSP completo. Siguen pendientes la
resolución exacta de scopes/shadowing, el tipo de cada expresión intermedia
(`make().field.method()`), la propagación del tipo de retorno por cadenas y la
navegación a la declaración del miembro.

### Verificación

```text
cd compiler
cargo test --quiet
65 passed; 0 failed

node --check vscode-ostrin/language-features.js
node --check vscode-ostrin/extension.js
```

El `cargo fmt --check` global continúa mostrando diferencias de formato
preexistentes en muchos archivos no relacionados; no se hizo un formateo
masivo para no mezclar cambios ajenos.

---

## 39. Alcance de bindings en el índice del editor — 2026-09-17

Se corrigió una limitación concreta del primer índice: si dos funciones tenían
un binding con el mismo nombre, VS Code podía elegir el tipo de la función que
aparecía antes en el archivo. El índice ahora conserva suficiente contexto
para seleccionar el binding de la función y del bloque actuales.

### Cambios realizados

- `EditorBinding` ahora publica `scope_depth`, además de su función, posición,
  tipo y archivo.
- El checker marca los parámetros en profundidad `0`, el cuerpo de una función
  en profundidad `1` y los bloques anidados en profundidades posteriores.
- `ostrinc --members --json` expone ese dato como `scopeDepth`.
- VS Code identifica la función que contiene el cursor mediante sus
  declaraciones y llaves, filtra bindings de otras funciones y prioriza el
  binding del bloque más interno.
- El hover aplica la misma selección de alcance, por lo que tampoco muestra
  el tipo de una variable homónima de otra función.

### Verificación

La prueba de integración de miembros comprueba bindings de `main`, `double` y
`generics.main`; además, una prueba de humo del proveedor de VS Code usa dos
funciones con `numbers` y confirma que una recibe miembros de `List` y la otra
de `Map`. La suite continúa en **65 pruebas exitosas**.

La resolución sigue siendo deliberadamente ligera: el cálculo de llaves del
proveedor es una aproximación para archivos incompletos y todavía no modela
shadowing exacto, scopes por expresión ni cadenas de miembros.

---

## 40. Resolución de cadenas de miembros en el editor — 2026-09-17

El completado dejó de limitarse a `binding.`. El índice ahora conserva el tipo
de resultado de cada campo y método, y el proveedor puede avanzar por una
cadena de acceso para encontrar el tipo del siguiente receptor.

### Cambios realizados

- `MemberSymbol` conserva `result_type` y los parámetros genéricos del tipo
  propietario; el JSON los publica como `resultType` y `ownerGenerics`.
- Se añadieron metadatos para los miembros de `record`, `trait`, `impl` y para
  las colecciones y contenedores del núcleo (`List.map` devuelve `List<U>`,
  `Map.get` devuelve `Option<V>`, etc.).
- El proveedor de VS Code separa cadenas respetando paréntesis, corchetes y
  llaves, por lo que puede resolver accesos como:

  ```ostrin
  user.address.
  numbers.map(fn(x) { x * 2 }).
  ```

- Se sustituyen los argumentos conocidos (`List<Int>` convierte `T` en
  `Int`) antes de buscar los miembros del siguiente tipo.
- También se reconoce una función inicial con firma conocida, de modo que una
  cadena que comienza por `make_user().address.` puede usar el retorno de
  `make_user` cuando el índice lo conoce.

### Límite actual

La inferencia del editor sigue siendo intencionalmente conservadora: no ejecuta
el checker completo sobre cada expresión ni infiere los genéricos `U` de una
lambda. Cuando el tipo de retorno no puede resolverse, la cadena termina en el
último tipo conocido. La siguiente mejora natural será publicar tipos de
expresiones directamente desde el checker y soportar navegación a la
declaración del miembro.

### Verificación

La suite permanece en **65 pruebas exitosas**; se ampliaron las comprobaciones
del índice para validar `resultType` en miembros de colecciones y de un record.
También pasó el smoke test de VS Code para `numbers.map(...).` y los chequeos
de sintaxis JavaScript.

---

## 41. Deploys de Pages separados del CI — 2026-09-17

Se revisó la cantidad de ejecuciones de GitHub Actions. No era necesario
publicar GitHub Pages después de cada cambio del compilador: `pages.yml` estaba
escuchando cualquier `push` a `main`, aunque solo se modificara Rust o la
extensión.

- `ci.yml` conserva la validación en cada `push` a `main` y cada pull request.
- `pages.yml` ahora se activa automáticamente solo cuando cambia `website/**`
  o el propio workflow de Pages.
- `workflow_dispatch` se mantiene para publicar manualmente cuando haga falta.

Los deploys históricos no se eliminan, pero los siguientes avances del
compilador ya no generarán deploys de Pages innecesarios.
