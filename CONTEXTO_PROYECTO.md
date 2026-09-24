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

## 4. El compilador (`compiler/`, Rust, con `toml` y `serde_json`)

Proyecto Cargo normal: `cd compiler && cargo build`, binario `ostrinc`.

```
compiler/
├── Cargo.toml              (`toml` para ostrin.toml y `serde_json` para LSP)
├── src/
│   ├── main.rs              — CLI: --check, --tokens, --ast, --run, --json, --symbols, --members, --types, --stdin, --lsp, --help, --version
│   ├── lsp.rs               — servidor JSON-RPC sobre stdio: lifecycle, diagnósticos, hover, completion y definición
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
    └── examples.rs           — 69 pruebas de integración (invocan el binario compilado, comparan CLI y protocolo LSP)
```

### Cómo correrlo

```bash
cd compiler
cargo build
cargo test                                    # 67 pruebas, deben pasar todas
./target/debug/ostrinc archivo.ostrin         # solo verifica tipos
./target/debug/ostrinc --run archivo.ostrin   # verifica y ejecuta
./target/debug/ostrinc --ast archivo.ostrin   # imprime el AST
./target/debug/ostrinc --tokens archivo.ostrin # imprime los tokens
./target/debug/ostrinc --check --json archivo.ostrin # JSON Lines para editores
./target/debug/ostrinc --symbols --json archivo.ostrin # símbolos y firmas
./target/debug/ostrinc --members --json archivo.ostrin # miembros por tipo y bindings locales
./target/debug/ostrinc --types --json archivo.ostrin # tipos inferidos de expresiones
./target/debug/ostrinc --stdin --check --json --file archivo.ostrin # buffer no guardado
./target/debug/ostrinc --lsp                    # servidor LSP sobre stdio
```

En Windows, si `cargo`/`rustc` no están en el PATH de la sesión: `$env:Path += ";$env:USERPROFILE\.cargo\bin"` (rustup se instaló vía `winget install Rustlang.Rustup` durante esta sesión).

### Arquitectura de evaluación (para quien continúe el intérprete)

- **`Value`** (en `interpreter/mod.rs`): `Int, Float, Bool, Char, String, Quantity(f64, Dimension, unit_str), List(Rc<RefCell<Vec<Value>>>), Closure, Record(String, Rc<RefCell<Vec<(String,Value)>>>), EnumInstance(enum, variant, HashMap<String,Value>), Task, Channel, `MapState`/`SetState` con entradas en orden e índice hash para escalares, `Void`.
- **Mutabilidad/identidad**: `List`, `Record`, `Map`, `Set`, `Task` y `Channel` usan `Rc<RefCell<>>` cuando necesitan identidad mutable. `List.push(value)` y `List.remove_at(index)` modifican el storage compartido; un alias puede observar el cambio. El type-checker exige binding `mut` para `push`/`remove_at` (E1053), y también para `Map.set`/`Map.remove` y `Set.add`/`Set.remove`.
- **Errores de control de flujo** (`return`/`break`/`continue`) se implementan reutilizando el canal `Result<Value, RuntimeError>` de Rust — `RuntimeError::Return(v)` sube por `?` de forma natural hasta el punto que sabe capturarlo (llamada de función para `Return`, bucle para `Break`/`Continue`). Patrón limpio, vale la pena mantenerlo si se sigue extendiendo.
- **Operadores sobre tipos de usuario** pasan por `Interpreter::eval_binary`, que busca `impl` real vía `find_method`, y si no hay `impl` pero el tipo tiene `derive(Eq)`/`derive(Ord)`, sintetiza la comparación campo por campo usando el orden de declaración (`self.records[type_name].fields`). Los tipos incorporados (`Int`, `Quantity`, etc.) tienen una vía rápida aparte (`eval_binary_builtin`) que **no** pasa por traits — es una simplificación deliberada del intérprete, no lo que dicta el diseño.
- **Módulos**: `modules.rs` resuelve todo el grafo de imports, y luego hace una **pasada de reescritura de AST** que renombra cada símbolo declarado fuera del módulo de entrada con un prefijo (`modulo.path::nombre`) y reescribe cada referencia cruzada (`alias.miembro`) para apuntar directamente ahí — es, en efecto, un mini-linker.
- **Coherencia de traits entre módulos**: durante esa reescritura se conserva `module_path` en records, enums, traits e `impl`; los traits públicos también participan en imports explícitos. Esto permite que el type-checker aplique la regla `orphan` con el módulo real de definición, no con una suposición posterior.

---

## 5. Qué está probado (y cómo verificarlo)

`compiler/tests/examples.rs` tiene 69 pruebas. Cubren, con valores exactos esperados (no solo "no falla"):

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

---

## 42. Navegación básica a definiciones desde VS Code — 2026-09-17

Se cerró otro tramo del ciclo semántico del editor: los miembros declarados
por el usuario ya conservan ubicación de origen y VS Code puede abrir esa
declaración desde el acceso correspondiente.

### Cambios realizados

- `MemberSymbol` conserva `span` y `source_file`; el comando
  `ostrinc --members --json` publica `file`, `line` y `column` junto con
  `resultType` y `ownerGenerics`.
- Campos de `record`, variantes de `enum`, métodos de `trait` y métodos de
  `impl` quedan indexados con la ubicación de su declaración. Los miembros
  internos (`List`, `Map`, `Option`, etc.) no inventan una ubicación porque no
  proceden de un archivo de usuario.
- La extensión registra `DefinitionProvider` y resuelve miembros de accesos
  directos o encadenados; también permite saltar a un binding local o a una
  declaración top-level indexada.
- El hover muestra el tipo de retorno resuelto cuando los argumentos genéricos
  del receptor son conocidos.
- README, documentación web y estado del ecosistema fueron actualizados.

### Límite actual

La ubicación de un campo o variante apunta por ahora al span del `record` o
`enum` propietario porque esos nodos todavía no conservan spans individuales.
La siguiente mejora puede añadir posiciones precisas por miembro y navegación
semántica completa para expresiones, rename y referencias múltiples.

---

## 43. Sustitución de genéricos en hover y autocompletado — 2026-09-17

El editor ya no muestra únicamente la firma genérica declarada. Cuando conoce
el tipo del receptor, sustituye sus argumentos en la firma presentada:

- `List<Int>.push` aparece como `push(value: Int) -> Void`.
- `Map<String, Int>.get` puede mostrar `get(key: String) -> Option<Int>`.
- Los campos de `record<T>` aplican la misma sustitución cuando el binding
  tiene un tipo aplicado.

La lógica reutiliza `ownerGenerics`, `resultType` y los mismos reemplazos que ya
usa la resolución de cadenas, por lo que hover y completion no pueden divergir
en la forma de resolver el receptor.

Se añadió `vscode-ostrin/test-language-features.js` y el script `npm test` de
la extensión. La prueba verifica la firma sustituida de `List<Int>` en
autocompletado y hover. El siguiente paso queda en firmas más precisas para
genéricos de lambdas (`U`) y en referencias múltiples para navegación.

---

## 44. Inferencia contextual de lambdas en colecciones — 2026-09-17

Se mejoró el checker para que una lambda reciba el tipo esperado por el método
de colección donde se utiliza. Antes, una lambda se infería aislada con
parámetros `Unknown`; por eso expresiones válidas como
`numbers.map(fn(x) { x * 2 })` podían conservar un tipo incompleto aunque el
receptor ya fuera `List<Int>`.

### Cambios realizados

- `map` propaga el tipo del elemento a la lambda y conserva el tipo concreto
  que devuelve su cuerpo: `List<Int>.map(...)` ahora produce `List<Int>` cuando
  la transformación devuelve `Int`.
- `filter`, `find`, `any` y `all` contextualizan el parámetro de la lambda con
  el tipo del elemento (`Int` en una `List<Int>`), manteniendo sus retornos
  `List<T>`, `Option<T>` o `Bool`.
- `fold` usa además el tipo del acumulador inicial para tipar el segundo
  parámetro de la lambda: `fold(0, fn(acc, x) { acc + x })` analiza `acc` y
  `x` como `Int` y devuelve `Int`.
- Las cadenas de miembros se benefician de la mejora porque el tipo concreto
  de una expresión intermedia queda disponible para el siguiente acceso.
- Se ampliaron las pruebas del índice de bindings para verificar `doubled`,
  `evens`, `total` y `found` en `examples/collections.ostrin`.

### Verificación

La suite del compilador permanece en **65 pruebas exitosas**. El programa de
colecciones sigue ejecutándose y el índice semántico publica ahora
`doubled: List<Int>`, `evens: List<Int>`, `total: Int` y
`found: Option<Int>`.

---

## 45. Inferencia contextual para `Option` y `Result` — 2026-09-17

La misma estrategia se extendió a los contenedores algebraicos del núcleo. Las
lambdas que transforman o encadenan `Option<T>` y `Result<T, E>` ya reciben el
tipo correcto del valor que procesan.

### Cambios realizados

- `Option<T>.map` y `Option<T>.then` contextualizan sus lambdas con `T`.
- `Result<T, E>.map` y `Result<T, E>.then` contextualizan el valor con `T`.
- `Result<T, E>.map_err` contextualiza la lambda con el error `E`.
- Las expresiones intermedias conservan sus tipos aplicados, por ejemplo
  `Option<Int>.map(...) -> Option<Int>` y
  `Result<Int, String>.map_err(...) -> Result<Int, String>`.
- Se añadió `examples/option_result_types.ostrin` y una prueba de índice que
  verifica los bindings `mapped`, `chained`, `result_mapped` y `error_mapped`.

### Verificación

La suite del compilador queda en **66 pruebas exitosas**. También se mantienen
las pruebas de la extensión de VS Code y la validación de sintaxis JavaScript.

---

## 46. Reconocimiento instalable en Visual Studio Code — 2026-09-17

Se aclaró y reforzó el flujo real de uso en VS Code. Ostrin no puede ser
reconocido por VS Code únicamente por existir en el repositorio: primero hay
que instalar la extensión. Una vez instalada, la contribución de lenguaje ya
asocia automáticamente la extensión `.ostrin` con el identificador `ostrin`.

### Cambios realizados

- La extensión conserva el reconocimiento automático de `.ostrin`, el
  resaltado, los comandos, diagnósticos, hover, completado y navegación.
- Se documentó el empaquetado local mediante `@vscode/vsce` y la instalación
  del archivo `.vsix` resultante con `code --install-extension`.
- `extension.js` ahora busca automáticamente el compilador local en
  `compiler/target/debug` o `compiler/target/release` antes de recurrir a
  `ostrinc` en `PATH`. Esto permite usar la extensión desde el repositorio
  después de ejecutar `cargo build`.
- Se añadió `.vscodeignore` para que el paquete distribuible no incluya
  pruebas internas ni metadatos del repositorio.
- README, guía de la extensión y documentación web quedaron alineados con el
  procedimiento real de instalación.

### Respuesta concreta

VS Code empieza a detectar Ostrin inmediatamente después de instalar la
extensión. El reconocimiento de archivos no espera al LSP completo; el LSP
será la siguiente capa para resolver expresiones con mayor precisión, rename,
formateo y depuración.

El paquete local se verificó con `npx --yes @vscode/vsce package` y genera
`vscode-ostrin/ostrin-language-support-0.1.0.vsix` con el logo, gramática,
configuración, proveedores y comandos de Ostrin incluidos.

Los metadatos del paquete también quedaron preparados para Marketplace:
publisher `ostrin-project`, licencia explícita, repositorio, página de inicio,
issues, banner oscuro, precio gratuito, changelog y soporte.

---

## 47. Icono propio para archivos `.ostrin` — 2026-09-17

La primera instalación ya coloreaba correctamente la sintaxis, pero el
Explorador seguía mostrando el icono genérico del tema de archivos de VS Code.
Se añadió una identidad visual específica para los archivos Ostrin.

### Cambios realizados

- La contribución de lenguaje `ostrin` publica iconos para tema claro y oscuro.
- Los iconos son SVG ligeros basados en el símbolo oficial de Ostrin, con la
  parte superior clara y la parte inferior violeta.
- La asociación se hace directamente sobre el lenguaje `.ostrin`, por lo que
  no reemplaza los iconos de otros tipos de archivo ni obliga a cambiar todo el
  tema de iconos del usuario.
- El changelog de la extensión quedó actualizado.

Al reinstalar el `.vsix` actualizado y recargar VS Code, los archivos
`.ostrin` deben mostrar el símbolo de Ostrin en el Explorador, pestañas y
listas de archivos compatibles con iconos de lenguaje.

---

## 48. Referencias y renombrado con scope en VS Code — 2026-09-17

La extensión ganó una primera capa de navegación semántica sobre nombres
locales. Ya no se limita a saltar a una declaración: puede localizar todos
los usos de un binding y preparar un renombrado que respete la función y el
scope visible.

### Cambios realizados

- Se registró `ReferenceProvider` para `Shift+F12` / Find All References.
- Se registró `RenameProvider` para `F2` sobre bindings locales.
- El indexado distingue los símbolos top-level de los miembros y ahora carga
  ambos índices desde `ostrinc --symbols --json` y `ostrinc --members --json`.
- La búsqueda ignora cadenas y comentarios de una línea para no modificar
  texto que solo coincide accidentalmente con el nombre.
- El renombrado usa la resolución de binding visible y `scopeDepth`, evitando
  renombrar una variable externa cuando existe otra con el mismo nombre en un
  scope interior.
- La extensión pasó a la versión `0.1.1`; README, changelog y web reflejan la
  nueva superficie.

### Límite actual

Esta primera implementación trabaja sobre el archivo abierto y bindings
indexados por el compilador. El siguiente salto será resolver referencias
entre módulos y expresiones completas con un índice persistente tipo LSP.

### Verificación

El smoke test de VS Code verifica tres referencias y tres reemplazos para un
binding local. También pasan la sintaxis JavaScript y las **66 pruebas** del
compilador.

---

## 49. Identidad de Ostrin en las estadísticas de GitHub — 2026-09-17

Se investigó por qué la barra de lenguajes del repositorio muestra Rust. No es
un error del proyecto: GitHub Linguist clasifica el código según su catálogo
oficial y actualmente no contiene una entrada para Ostrin. Por eso reconoce
los archivos Rust del compilador, mientras que `.ostrin` todavía no aparece
como una categoría propia.

### Decisión

- No se marcará Rust como vendored, generado o documentación: eso falsearía la
  naturaleza del repositorio, porque Rust sí es la implementación del
  compilador e intérprete.
- El README ahora incluye un mapa explícito entre programas Ostrin,
  implementación Rust y tooling de VS Code.
- La solución definitiva será preparar una propuesta para `github-linguist`
  con extensión `.ostrin`, color, identificador y scope de TextMate cuando el
  lenguaje tenga suficiente adopción pública para cumplir sus criterios.

GitHub documenta que `linguist-language` solo puede clasificar nombres que
existen en su catálogo; un nombre personalizado aún no entra en las
estadísticas. Esta aclaración evita presentar una alteración cosmética como si
fuera reconocimiento oficial del lenguaje.

---

## 50. Referencias entre archivos en el editor — 2026-09-17

La extensión avanzó desde referencias únicamente locales hacia una primera
resolución entre archivos del workspace. Al actualizar el índice semántico,
`extension.js` combina ahora la salida de `ostrinc --symbols --json` y
`ostrinc --members --json`, reúne los índices de los documentos Ostrin abiertos,
y `language-features.js` puede abrir los archivos indexados para localizar usos
de un símbolo top-level único.

### Comportamiento seguro

- Las variables locales siguen resolviéndose por función, profundidad de scope y
  binding visible.
- Los símbolos top-level con un nombre único dentro de los documentos
  indexados pueden devolver referencias y renombrados en más de un archivo.
- Si varios módulos exponen el mismo nombre corto, la extensión no adivina la
  propiedad: mantiene la operación local hasta que el índice publique la
  identidad completa del módulo.
- Las ediciones de renombrado usan la URI real de cada archivo, por lo que no
  se aplican accidentalmente al documento abierto cuando hay referencias
  externas.

### Entrega y verificación

La extensión pasa a `0.1.2`. Se actualizaron README, changelog y la página de
documentación con el nuevo nombre del paquete `.vsix`. Se verificaron el smoke
test de JavaScript, la sintaxis de los dos archivos de la extensión, el
empaquetado VSIX y las **66 pruebas** del compilador Rust.

Este es todavía un índice semántico ligero de documentos abiertos, no un
servidor LSP completo. El siguiente bloque técnico es persistir la resolución
de tipos a nivel de expresión y convertirla en un servicio LSP completo para
que diagnósticos, hover, completion y referencias compartan una misma fuente
de verdad.

---

## 51. Tipos de expresiones para herramientas del editor — 2026-09-17

Se cerró la primera parte de esa siguiente capa. El AST ahora conserva una
envoltura `Expr::Located` alrededor de las expresiones que nacen del parser.
La envoltura no cambia la semántica: el checker y el intérprete la atraviesan,
pero conserva la línea y columna iniciales para diagnósticos y herramientas.

### Salida del compilador

Se añadió `ostrinc --types --json`. Cada línea publica un registro con:

- `kind: "expression"`;
- tipo inferido renderizado (`Int`, `Option<Int>`, `Result<Int, String>`, etc.);
- función contenedora;
- archivo, línea y columna de origen.

Los errores del checker siguen saliendo por `--check --json`; la nueva salida es
metadata semántica independiente para que un editor pueda continuar mostrando
tipos aun durante una edición incompleta.

### Integración en VS Code

El índice de la extensión combina ahora símbolos, miembros, bindings y tipos de
expresiones. El hover conserva la prioridad de miembros, bindings y símbolos;
cuando el cursor no coincide con uno de ellos puede mostrar el tipo inferido de
la expresión más cercana en esa línea.

La extensión pasa a la versión `0.1.3`; README, changelog, instrucciones de
instalación y la página pública usan el nuevo nombre del paquete `.vsix`.

### Compatibilidad y verificación

Se añadió `Expr::unlocated()` para que módulos, checker, intérprete y análisis
de variables mantengan sus decisiones existentes. Pasan **67 pruebas** del
compilador, el smoke test de VS Code y la comprobación sintáctica de la
extensión.

La precisión actual es la posición inicial de cada expresión, no todavía un
rango completo de inicio-fin. El siguiente refinamiento será persistir este
índice como un servicio LSP y añadir debugging.

---

## 52. Formatter inicial para VS Code — 2026-09-17

La extensión registra ahora un `DocumentFormattingEditProvider` para archivos
`.ostrin`. El formatter es deliberadamente conservador: elimina espacios al
final, normaliza la indentación a cuatro espacios según `{` y `}`, y conserva
el contenido de strings, comentarios y operadores.

No intenta reescribir la expresión ni imponer una estética que pudiera cambiar
la interpretación de los saltos de línea significativos de Ostrin. Devuelve
una edición de documento completa solo cuando el texto cambia, por lo que
`Format Document` queda integrado sin añadir un comando propietario.

La prueba de VS Code cubre tanto el texto formateado como la edición que
recibiría el editor. La extensión pasa a `0.1.4`; el siguiente bloque de
herramientas será persistir el estado semántico como un LSP y añadir debugging.

---

## 53. Ciclo de vida del índice semántico — 2026-09-17

La extensión dejó de tratar el índice semántico como datos permanentes sin
invalidación. Cada documento tiene ahora una generación de índice:

- al editar un archivo `.ostrin`, se elimina inmediatamente su información
  anterior para no mostrar tipos o referencias obsoletos;
- al guardar, se ejecutan de nuevo `--members`, `--symbols` y `--types`;
- si una compilación anterior termina después de una nueva, su resultado se
  descarta por generación y no puede sobrescribir el estado reciente;
- al cerrar el documento, su índice se elimina;
- los índices se combinan únicamente dentro del mismo workspace raíz.

Esto mantiene persistencia durante la sesión sin fingir que ya existe un LSP
completo. La extensión pasa a `0.1.5`. Se verificaron el smoke test de VS Code,
la sintaxis JavaScript y las **67 pruebas** del compilador; el siguiente bloque
grande es el protocolo LSP completo y debugging.

---

## 54. Rangos completos de expresiones — 2026-09-17

La metadata de tipos dejó de publicar únicamente la posición inicial. Cada
`Expr::Located` conserva ahora un `SourceRange` con `start` y `end`, calculado
por el parser a partir de los tokens consumidos. `--types --json` publica esos
datos como `line`, `column`, `endLine` y `endColumn`.

La extensión usa el rango para elegir la expresión más pequeña que contiene el
cursor. Por eso un hover puede distinguir una llamada interna de la expresión
completa que la contiene, y devuelve un `Hover.range` preciso. Se añadieron
aserciones Rust y JavaScript para comprobar tanto los límites publicados como
el rango que recibe VS Code.

La precisión de rangos está lista para el siguiente paso: exponer las mismas
operaciones mediante el protocolo LSP estándar y añadir debugging.

---

## 55. Ayuda de firmas de llamadas en VS Code — 2026-09-17

La extensión añade ahora `SignatureHelpProvider` para que una llamada Ostrin
muestre su firma mientras se escribe. Los disparadores son `(` y `,`, y la
ayuda identifica:

- funciones globales publicadas por `ostrinc --symbols --json`;
- métodos del receptor usando la misma resolución de tipos que completion y
  hover;
- sustitución de genéricos del receptor, por ejemplo `List<Int>` convierte
  `push(value: T) -> Void` en `push(value: Int) -> Void`;
- el argumento activo después de comas, paréntesis anidados, listas y bloques.

La información se presenta con `SignatureInformation` y
`ParameterInformation`, por lo que VS Code puede resaltar el parámetro actual
de una llamada y mostrar la documentación contextual. La extensión pasa a la
versión `0.1.7`; se añadieron pruebas de firma al smoke test de JavaScript y
se actualizaron README, changelog, roadmap y la web.

Este paso sigue siendo una integración directa con VS Code, no un servidor LSP
independiente. El siguiente bloque grande continúa siendo extraer un servicio
semántico persistente con protocolo LSP y después conectar depuración.

---

## 56. Diagnósticos en tiempo real sobre texto no guardado — 2026-09-17

El compilador ahora acepta una fuente por entrada estándar:

```text
ostrinc --stdin --check --json --file archivo.ostrin
```

El modo conserva el mismo lexer, parser, recuperación de errores y checker que
la ruta normal de archivos. `--file` solo asocia la fuente temporal con una
ubicación para que los diagnósticos mantengan su archivo, línea y columna; no
escribe ese contenido en disco. El JSON permanece silencioso cuando la fuente
es válida, lo que permite consumirlo como proceso hijo desde un editor.

La extensión de VS Code conecta esa entrada con un ciclo de diagnóstico en
tiempo real:

- cada edición de un `.ostrin` programa una comprobación con debounce;
- el contenido actual se envía por `stdin`, sin guardar automáticamente el
  documento;
- los diagnósticos viejos se limpian mientras llega el nuevo resultado;
- cada documento tiene una generación de diagnóstico y los procesos tardíos
  se descartan, evitando que una edición antigua sobrescriba la actual;
- al cerrar un documento se cancela el temporizador y se invalidan sus
  resultados pendientes;
- `ostrin.diagnosticsOnType` permite desactivar la comprobación y
  `ostrin.diagnosticsDebounceMs` permite ajustar el retraso entre 100 y 2000 ms.

Se añadió una prueba de integración del modo `stdin`; la suite del compilador
pasa a **68 pruebas**. La extensión pasa a `0.1.8` y se regeneró el VSIX. Este
es el primer paso de un servicio semántico que puede trabajar con buffers en
memoria; el siguiente tramo será reutilizar esta sesión para un servidor LSP
con `initialize`, `didOpen`, `didChange`, `publishDiagnostics`, hover y
completion por el protocolo estándar.

---

## 57. Primer backend LSP persistente — 2026-09-17

El compilador incorpora `ostrinc --lsp`, un servidor JSON-RPC sobre `stdio`
con framing `Content-Length`. No es una simulación de documentación: mantiene
documentos abiertos en memoria y ejecuta el mismo lexer, parser y checker que
la CLI.

### Protocolo implementado

- `initialize` / `initialized` con capacidades declaradas;
- `shutdown` / `exit`;
- `textDocument/didOpen`, `didChange`, `didSave` y `didClose`;
- `textDocument/publishDiagnostics` con códigos Ostrin y rangos LSP cero-based;
- `textDocument/hover` para miembros, bindings, símbolos y expresiones
  inferidas;
- `textDocument/completion` para símbolos, miembros y bindings;
- `textDocument/definition` para declaraciones indexadas.

La extensión incluye `lsp-client.js`, un cliente stdio sin dependencia de un
servidor externo. Al abrir un documento disponible, inicia un proceso
persistente de `ostrinc --lsp`, envía los cambios completos del buffer y
consume las notificaciones de diagnósticos. Mientras el servidor se inicia o
si el ejecutable no está disponible, conserva el modo `--stdin` con debounce
como respaldo. Los proveedores directos de VS Code siguen activos para evitar
regresiones mientras la semántica se migra al protocolo estándar.

La prueba de integración abre una sesión LSP completa, negocia capacidades y
verifica que un error de tipo llegue como `publishDiagnostics` con el rango
correcto. La suite del compilador pasa a **69 pruebas**. La extensión pasa a
`0.2.0`, se incluye el nuevo cliente en el VSIX y la web deja de presentar el
transporte LSP como trabajo inexistente.

Todavía falta completar la resolución de workspace/imports dentro de la sesión,
semantic tokens, referencias/rename por protocolo y debugging. Esa es la
siguiente expansión grande del servidor.

---

## 58. Workspace real, referencias/rename y semantic tokens en el LSP — 2026-09-17

Se cerró el pendiente que dejó la sección 57: el servidor ya no analiza cada
documento en aislamiento.

### Resolución de imports/workspace con buffers en memoria

`modules::load_project` (nueva función; `load_project_with_deps` queda como
envoltorio con overrides vacíos) acepta un mapa `HashMap<PathBuf, String>` de
sustituciones. `lsp.rs` construye ese mapa a partir de **todos** los
documentos abiertos (rutas canonicalizadas) y, cada vez que cualquiera de
ellos cambia, vuelve a resolver el proyecto completo de ese documento —
imports, cadena de `pub import`, dependencias de `ostrin.toml` — usando el
texto no guardado de cualquier archivo abierto en vez del último guardado en
disco. Un archivo que solo se importa (nunca se abre) sigue leyéndose de
disco con normalidad. Los diagnósticos resultantes se agrupan por el archivo
real al que pertenecen (`ModuleDiagnostic.file` / `TypeError.source_file`) y
se publican con `textDocument/publishDiagnostics` en la pestaña que
corresponde, no solo en la que se editó.

El servidor mantiene `index: HashMap<ruta, FileCache>` (símbolos, miembros,
bindings, expresiones) que se repuebla por archivo cada vez que una resolución
lo toca, así que hover/completion/definition ahora buscan en **todo el
proyecto conocido**, no solo en el documento activo.

### Referencias, rename y signature help nativos por protocolo

- `textDocument/references` y `textDocument/rename`: si la palabra bajo el
  cursor es un binding local (aparece en `bindings` del archivo propio), la
  búsqueda queda contenida a ese archivo; si es un símbolo global (función,
  récord, campo, método...) se busca por texto en **todos los documentos
  abiertos**. Es una búsqueda léxica con límites de palabra, no semántica —
  limitación documentada, igual de honesta que las anteriores.
- `textDocument/signatureHelp`: reconstruye la llamada activa escaneando hacia
  atrás desde el cursor (profundidad de paréntesis, comas de nivel superior) y
  arma las firmas a partir de `Symbol`/`MemberSymbol` ya indexados.
- `textDocument/semanticTokens/full`: legend propio
  (`function,type,enum,enumMember,interface,property,method,variable`);
  etiqueta cada ocurrencia léxica de un nombre indexado con su tipo. También
  es una aproximación léxica, no basada en el AST resuelto en esa posición.

### Extensión de VS Code conectada de verdad

Hasta ahora el servidor persistente solo alimentaba diagnósticos: hover,
completion, definition, signature help, references y rename seguían corriendo
100% del lado del cliente (invocando `ostrinc --symbols/--members/--types` por
archivo). Se añadieron métodos (`hover`, `definition`, `completion`,
`signatureHelp`, `references`, `rename`, `semanticTokens`) a
`OstrinLanguageClient` en `lsp-client.js`, y cada proveedor en `extension.js`
ahora intenta el servidor primero y solo cae al camino antiguo si el servidor
no está listo o la petición falla. Se registró además un
`DocumentSemanticTokensProvider` nuevo (no existía ninguno antes).

### Pruebas

Se añadió `compiler_lsp_resolves_imports_across_open_documents`, que abre
`examples/proj1/main.ostrin` y `examples/proj1/physics/units.ostrin` como
documentos separados, verifica hover/references/signatureHelp/semanticTokens/
rename cruzando archivos, y luego edita `units.ostrin` en memoria (quitando un
`pub`) para comprobar que el diagnóstico de símbolo privado aparece en
`main.ostrin` sin tocar el disco — la prueba directa de que la resolución usa
el buffer vivo. La suite del compilador pasa a **70 pruebas**; `cargo build`
no deja warnings nuevos. La extensión pasa a `0.3.0` y se regeneró el VSIX.

Limitaciones que quedan explícitas para el siguiente tramo: referencias/rename
solo ven archivos que estén *abiertos* (no recorren el árbol de archivos del
workspace en disco), y semantic tokens es léxico por nombre indexado, no por
la posición resuelta del AST (puede sobre-etiquetar un identificador que
coincide de nombre con un símbolo de otro alcance). El siguiente bloque
grande sigue siendo, como ya se anotó antes, depuración (`debug adapter`) y,
si se quiere cerrar del todo esta brecha, un recorrido de workspace en disco
para referencias/rename fuera de los documentos abiertos.

---

## 59. Referencias/rename recorren el workspace en disco, no solo lo abierto — 2026-09-17

Cerró la limitación que quedó anotada al final de la sección 58: hasta ahora
`textDocument/references` y `textDocument/rename` solo veían texto de
documentos que el editor tuviera efectivamente abiertos.

`lsp.rs` guarda ahora `Server.root`, tomado de `rootUri` (o `rootPath`) en el
mensaje `initialize` — el cliente ya lo enviaba desde que existe el servidor
persistente, así que no hizo falta tocar `lsp-client.js`. `workspace_files()`
combina los documentos abiertos (su texto en memoria, siempre con prioridad)
con un recorrido recursivo (`walk_ostrin_files`, con límites sensatos:
ignora `.git`, `target`, `node_modules`, `.vscode`) de todo `.ostrin` bajo esa
raíz, leyendo del disco cualquier archivo que no esté abierto. Referencias y
rename usan esa lista en vez de `server.documents` para la búsqueda de
símbolos globales; los bindings locales siguen restringidos al archivo propio,
sin cambios.

Nueva prueba `compiler_lsp_finds_references_in_unopened_workspace_files`:
inicializa con `rootUri` apuntando a `examples/proj1`, abre solo `main.ostrin`
(nunca `physics/units.ostrin`) y comprueba que tanto `references` como
`rename` sobre `to_kelvin` incluyen la declaración leída directamente de
`units.ostrin` desde disco. Suite del compilador: **71 pruebas**, sin
warnings. Extensión → `0.3.1`, VSIX regenerado.

Con esto la brecha de la sección 57 queda cerrada por completo. Lo que sigue
pendiente en el plan de continuación es el mismo de siempre: un backend de
compilación real (LLVM u otro) es la pieza más grande sin empezar; para
herramientas de editor, lo siguiente sería un debug adapter (DAP) — hasta
ahora nunca se ha tocado ese terreno.

---

## 60. Depurador real: `ostrinc --dap` — 2026-09-17

Se le preguntó al usuario cuál de los dos frentes grandes pendientes atacar
(backend de compilación real vs. debug adapter) y se eligió depuración, por
seguir el mismo arco de las últimas sesiones (herramientas de editor) sin
comprometerse todavía a un proyecto del tamaño de un backend LLVM.

### Cómo se resolvió pausar un intérprete síncrono sin hilos

El intérprete es un tree-walking interpreter de un solo hilo, y sus valores
(`Rc<RefCell<...>>`) no son `Send` — mover la ejecución a un hilo de SO
aparte (para que otro hilo maneje el protocolo DAP mientras el programa
corre) habría exigido el mismo refactor a `Arc<Mutex<>>` que ya se descartó
para concurrencia real (documento 10, sección "Problem Solving" de sesiones
anteriores). En vez de eso, pausar significa literalmente **no volver**: el
propio hilo que está ejecutando el programa del usuario, al llegar a un punto
de pausa, entra en un bucle bloqueante que lee mensajes DAP de stdin y los
responde directamente con el estado vivo del intérprete (call stack, entornos)
hasta que llega `continue`/`next`/`stepIn`/`stepOut`/`disconnect`. No hay
hilos, ni corutinas, ni un rediseño del evaluador a máquina de estados — la
propia pila de llamadas de Rust (la recursión de `eval_block`/`eval_expr`) es
la pila de la sesión de depuración.

### Lo que se añadió

- `compiler/src/protocol.rs`: el framing `Content-Length` que ya usaba
  `lsp.rs` se extrajo a un módulo compartido (también lo usa `dap.rs` y el
  propio bucle de pausa del intérprete).
- `compiler/src/interpreter/mod.rs`: nuevo `CallFrame` (nombre, archivo,
  línea, entorno base y entorno actual) empujado/sacado solo en límites
  reales de llamada a función; `Debugger` (breakpoints por archivo,
  modo de paso, transporte stdio); `RuntimeError::Terminated` para
  desenrollar limpio ante `disconnect`/`terminate`. El punto de enganche es
  `eval_block`: antes de cada sentencia (y también antes de evaluar la
  expresión final de un bloque sin sentencia final explícita — el caso de
  `for n in fib { print(n) }`, donde `print(n)` se parsea como `tail`, no
  como sentencia, y sin este segundo enganche el breakpoint nunca se
  disparaba) se decide si hay que pausar. `print()` redirige su salida a un
  evento `output` de DAP en vez de `stdout` real mientras hay un depurador
  conectado.
- `compiler/src/dap.rs`: maneja el protocolo previo al lanzamiento
  (`initialize`, `launch`, `setBreakpoints`, `configurationDone`), carga y
  tipa el proyecto exactamente igual que `--run`, y entrega la sesión al
  intérprete.
- Soportado por protocolo: breakpoints por línea, `stopOnEntry`, `continue`,
  `next` (step over), `stepIn`, `stepOut`, `pause`, `threads`, `stackTrace`,
  `scopes`, `variables` (variables locales alcanzables desde el entorno del
  frame, con `push`/`derive` incluidos porque son valores reales), `evaluate`
  (ejecuta la expresión con el parser/evaluador real del lenguaje contra el
  entorno vivo del frame pausado — no un mini-lenguaje aparte), `disconnect`/
  `terminate`.
- Extensión VS Code: tipo de depurador `ostrin` registrado
  (`registerDebugAdapterDescriptorFactory` lanza `ostrinc --dap` como
  proceso hijo, igual que ya se hacía para `--lsp`), un
  `DebugConfigurationProvider` que por defecto depura el archivo activo, y
  un snippet de `launch.json` ("Ostrin: Debug current file").

### Limitaciones documentadas

Los breakpoints solo se pueden fijar antes de lanzar o mientras el programa
está pausado (no hay forma de recibirlos mientras el programa corre entre
breakpoints, porque el hilo no está leyendo stdin en ese momento — limitación
compartida con muchos adaptadores de referencia simples). Las funciones
lambda (`call_callable`) no empujan su propio `CallFrame`: sus sentencias se
atribuyen al frame nombrado que las llamó. `evaluate` solo puede referirse a
variables visibles en el frame activo, no puede llamar a funciones con
efectos secundarios sobre el "programa real" de forma distinta a como el
propio programa ya las llamaría (no hay sandboxing especial, es el mismo
evaluador).

### Pruebas

Dos pruebas de integración nuevas sobre el proceso `ostrinc --dap` real (no
mocks): `compiler_dap_hits_breakpoints_and_reports_locals` pone un breakpoint
en la única línea del cuerpo de un `for` (`examples/fibonacci.ostrin`),
verifica que se dispara exactamente 10 veces, que `stackTrace`/`variables`
exponen `n` y `fib`, que `evaluate("n")` en la primera parada da `"0"`, y que
la secuencia completa de `print()` llega como eventos `output` en el orden
correcto antes de `terminated`/`exited`. `compiler_dap_stops_on_entry_steps_and_disconnects_cleanly`
prueba `stopOnEntry`, un `next` que avanza exactamente una sentencia, y que
`disconnect` corta el programa antes de que imprima nada. Suite del
compilador: **73 pruebas**, sin warnings nuevos. Extensión → `0.4.0`, VSIX
regenerado.

Con debug y LSP completos, el único frente grande realmente sin empezar en
todo el proyecto es el backend de compilación real (LLVM u otro).

---

## 61. Primer backend de compilación real: `ostrinc --emit-c` / `--compile` — 2026-09-17

Se preguntó al usuario qué enfoque tomar para el backend de compilación real:
LLVM (vía `inkwell`), transpilar a Rust, o transpilar a C. Se eligió **C**,
por ser el camino con menos fricción de dependencias (el sistema ya tenía
`gcc` de MinGW disponible) y el más fácil de inspeccionar/depurar cuando algo
sale mal — el mismo criterio que llevó a preferir TOML sobre un formato propio
para `ostrin.toml`, o simulación sobre hilos reales para concurrencia:
resolver primero el problema con la herramienta más simple que sea honesta
sobre sus límites.

### Alcance real, no fingido

Igual que el primer LSP (sección 57) o el primer DAP (sección 60) no
pretendieron cubrir todo el protocolo desde el día uno, este backend no
pretende compilar todo Ostrin. Lo que compila de verdad, a un ejecutable
nativo, sin pasar por el intérprete: funciones simples sobre
`Int`/`Float`/`Bool`/`String`, recursión, `if`/`while`/`for <rango>`,
operadores aritméticos/de comparación/lógicos, concatenación e igualdad de
`String`. Lo que NO compila — y falla con un mensaje explícito señalando de
vuelta al intérprete, no en silencio ni con un resultado incorrecto —:
records, enums, traits, genéricos, `Quantity`/unidades, closures,
colecciones, pattern matching, `spawn`/canales.

### Cómo se resolvió representar `if`/bloques como expresión sin generar
código incorrecto

Ostrin, como Rust, permite que un bloque termine en una expresión sin `;` que
se convierte en su valor (`if cond { a } else { b }` es una expresión válida
en cualquier posición). C no tiene eso. En vez de reescribir el AST para
eliminar esa forma (lo que habría exigido duplicar cada bloque o introducir
variables temporales por todas partes), el generador usa **expresiones de
sentencias de GNU** (`({ ...; valor; })`), soportadas por gcc y clang aunque
no por MSVC — de ahí que `find_c_compiler` busque específicamente
`cc`/`gcc`/`clang`, nunca `cl`. Cuando un `if` se usa como sentencia pura (el
caso más común, sin capturar su valor) el generador emite un `if`/`else` de C
normal en vez de la forma de expresión, para que el código generado sea
legible en el caso típico.

### El bug real que salió al probarlo contra un ejemplo existente

Al generar código para `for n in fib { print(n) }` real, hubo que redescubrir
(otra vez) la misma trampa que ya había mordido al DAP en la sección 60:
`print(n)`, al ser la única línea del cuerpo, se parsea como la expresión
`tail` del bloque, no como una sentencia. El primer intento de generar el
cuerpo de un `for`/`while` solo recorría `block.stmts` e ignoraba `tail`,
así que un bucle de una sola línea compilaba a un cuerpo vacío. Corregido en
`gen_block_stmts`, que ahora también emite el `tail` (descartando su valor,
ya que en posición de sentencia no se necesita).

### Inferencia de tipos propia, deliberadamente separada de `typeck`

El generador no reutiliza `typeck::Ty` — hace su propia inferencia mínima
(`CType`: `Int`/`Float`/`Bool`/`Str`/`Void`) porque para cuando corre, el
programa **ya pasó** el verificador de tipos real; no necesita validar nada,
solo necesita saber qué tipo primitivo concreto tiene cada expresión para
elegir el tipo de C correcto y el especificador de `printf` adecuado
(`print` no tiene una forma sintáctica con formato en Ostrin — el generador
elige `%lld`/`%g`/`%s` según el tipo estático de su único argumento).

### CLI

- `ostrinc --emit-c file.ostrin` imprime el C generado a stdout (o a
  `--out ruta` si se da);
- `ostrinc --compile file.ostrin [--out ruta]` genera el C a un archivo
  temporal, invoca `$OSTRIN_CC` o el primero que funcione entre
  `cc`/`gcc`/`clang`, y produce un ejecutable nativo real (por defecto junto
  al archivo fuente, con el mismo nombre base).
- Se corrigió de paso un bug de parseo de argumentos preexistente: la
  detección del archivo de entrada tomaba el primer argumento sin `--` como
  ruta, así que `--out valor` colocado antes del archivo `.ostrin` hacía que
  `valor` se confundiera con la ruta de entrada. Ahora se reconocen
  explícitamente `--file`/`--out` como flags que consumen el siguiente
  argumento.

### Pruebas

`examples/native_fibonacci.ostrin` (recursión + `for` + `if`-expresión) y
`examples/native_strings.ostrin` (concatenación de `String`, `while`, `Bool`)
se compilan de verdad con `--compile` y el binario resultante se ejecuta como
proceso aparte, comparando su `stdout` byte a byte (normalizando `\r\n` de
Windows) contra la secuencia esperada — no se compara contra el intérprete,
se verifica el resultado real del binario nativo. Un tercer test confirma que
`--emit-c` sobre un programa con `enum`/`impl` (`shapes.ostrin`) falla con un
mensaje que menciona `--run`. Los dos tests que invocan un compilador de C de
verdad se saltan con un aviso (no fallan) si la máquina no tiene
`gcc`/`clang`/`cc`, para no romper la suite en un entorno sin toolchain de C.
Suite del compilador: **77 pruebas**, sin warnings nuevos.

### Limitaciones explícitas

Sin manejo de memoria real (`String` nunca se libera — aceptable para
programas cortos, no para uno de larga duración); sin `break`/`continue` con
valor; sin rangos con paso (`a to b by n`); solo llamadas directas a
funciones nombradas (no valores de función); un solo argumento en `print`.
Documentado explícitamente, en el mismo espíritu que el resto del proyecto:
mejor un subconjunto pequeño que funciona de verdad y dice claramente qué le
falta, que fingir cobertura completa.

Con esto, los tres frentes grandes que quedaban (LSP completo, depurador,
backend de compilación real) tienen al menos una primera versión real y
probada. Lo que sigue, si se quiere seguir creciendo el backend nativo, es
ampliar el subconjunto soportado (records simples primero, probablemente,
ya que no requieren dispatch dinámico).

---

## 62. Records reales en el backend nativo — 2026-09-17

Se siguió la recomendación que cerraba la sección 61: el backend de C ahora
compila `record` de verdad — sin `impl`/métodos todavía, que es justo el
límite que no requiere resolver dispatch dinámico.

### Cómo se preservó la identidad por referencia sin copiar el modelo del intérprete

El intérprete representa `Record` como `Rc<RefCell<Vec<(String, Value)>>>`
precisamente porque dos bindings que apuntan al mismo record deben ver la
mutación del otro (documento 11). El backend de C reproduce esa semántica de
la única forma que tiene sentido sin un GC: cada record es **siempre** un
puntero a memoria reservada con `malloc` — nunca una copia por valor, nunca
un `struct` en la pila. `Point* p = ...; Point* p2 = p; p2.x = ...` en Ostrin
se traduce a asignaciones de puntero en C, así que `p` y `p2` siguen viendo
el mismo bloque de memoria, igual que en el intérprete. Nada libera esa
memoria — aceptable para los programas cortos que este backend apunta a
compilar, documentado explícitamente como límite, no como descuido.

Para que dos records puedan referenciarse entre sí como campos (o incluso
formar un ciclo) sin pelear con el orden de declaración de C, primero se
emiten TODOS los `typedef struct X X;` como declaraciones adelantadas, y
recién después los cuerpos `struct X { ... };` completos — un puntero a un
tipo incompleto es válido en C, así que el orden de los cuerpos entre sí deja
de importar.

### Un bug real, otra vez por la misma causa de fondo

Al probar contra un programa con records anidados apareció un tercer caso de
la misma familia de bugs que ya había mordido al DAP (sección 60) y a este
mismo backend (sección 61): Ostrin no tiene palabra clave `let` — `nombre =
valor` sin `mut` se parsea **siempre** como `Stmt::Assign`, sin importar si
`nombre` es una variable nueva o una ya existente; es el intérprete quien
decide en tiempo de ejecución cuál de las dos cosas es, mirando si el entorno
ya conoce ese nombre (`Env::assign`, con caída a `define` si no existe). El
generador de C asumía que `Stmt::Assign` siempre reasignaba una variable C ya
declarada, así que `linea = punto` como primer uso de `linea` fallaba con
"no type recorded for 'linea'". Corregido replicando la misma regla que ya
usa el intérprete: si `self.lookup(nombre)` no encuentra nada, se declara una
variable de C nueva en vez de asumir que ya existe.

### Qué rechaza, y por qué el rechazo importa tanto como lo que sí compila

Un `impl` que existe en el programa pero cuyos métodos nadie llama se
**ignora** silenciosamente (no se traduce, pero tampoco hace fallar la
compilación) — porque una llamada real a un método (`valor.metodo(...)`) ya
falla por su cuenta en `gen_call`, que solo acepta un nombre de función
suelto como callee. Lo que si se rechaza explícitamente es cualquier
operador (`+`, `==`, etc.) aplicado a dos records: sin eso, el generador
habría emitido `==` de C sobre los punteros (comparación de identidad) en
vez de la igualdad estructural que pide `derive(Eq)`/`impl Eq` — un bug de
corrección silencioso, no solo una limitación. Se prefirió fallar con un
mensaje claro ("operators on records aren't supported yet") antes que
compilar algo que se ve bien pero da resultados distintos a los del
intérprete.

### Pruebas

`examples/native_records.ostrin` (dos records, uno anidado dentro del otro,
mutación de un campo `mut` a través de su binding, un record pasado por
identidad a otra función) se compila y ejecuta de verdad, comparando su
salida byte a byte tanto contra lo esperado como — ya verificado a mano —
contra la salida del intérprete para el mismo programa. Un segundo test
reutiliza `examples/traits.ostrin` (que ya existía, con `impl Add`/`impl Eq`
sobre `Vector2`) para confirmar que el backend nativo rechaza el operador
`+`/`==` sobre records en vez de compilarlo mal. Suite del compilador:
**79 pruebas**, sin warnings nuevos.

Frontera actual del backend nativo: funciones y records simples (sin
genéricos, sin `impl`) sobre `Int`/`Float`/`Bool`/`String`, con recursión,
`if`/`while`/`for <rango>` y los operadores usuales. Todo lo que necesite
despacho dinámico (traits, `impl`, operadores sobre tipos definidos por el
usuario) sigue siendo terreno exclusivo del intérprete — ampliarlo más allá
de eso es un proyecto en sí mismo, no una extensión trivial de lo que ya
existe.

---

## 63. Métodos de `impl` en el backend nativo — 2026-09-17

Se continuó el mismo camino: la sección 62 dejó `record` sin métodos porque
"no requieren dispatch dinámico" era el límite natural; esta sección mueve
ese límite un paso más, a métodos que **tampoco** lo requieren.

### Por qué un método de `impl` no necesita ninguna forma de dispatch aquí

En Rust, una vtable existe para resolver en tiempo de ejecución CUÁL
implementación concreta corresponde a un `dyn Trait` cuyo tipo real no se
conoce en tiempo de compilación. Este backend no tiene `dyn Trait` ni
genéricos en ningún punto — así que el tipo concreto del receptor de
`valor.metodo(...)` (un `Record(nombre)`) **siempre** se conoce en el punto
de la llamada, sea el método de un `impl` inherente o de un `impl Trait for
Tipo`. Por eso no hizo falta ninguna infraestructura de despacho: cada
combinación (record, nombre de método) se resuelve una sola vez, durante la
generación, a un nombre de función de C único (`Record__metodo`), y la
llamada se convierte simplemente en pasar el puntero del receptor como
primer argumento — ni siquiera hace falta que el programa haya usado
`derive`/`impl Trait` en vez de un `impl` inherente, es exactamente la misma
mecánica en ambos casos.

`self`/`mut self` se resuelven como cualquier otro parámetro, salvo que su
tipo declarado es literalmente `Self` (así lo produce el parser — ver
`parser/mod.rs`, línea ~283), que se traduce al record concreto que está
implementando el método (`map_method_type`). El propio identificador `self`
dentro del cuerpo no necesitó ningún caso especial: es sólo un parámetro más
con ese nombre.

### Qué queda deliberadamente fuera, y por qué no rompe nada

Un `impl` cuyo método nadie llama, o un método genérico, o un `impl` sobre
un tipo que el backend no representa (un `enum`, por ejemplo) simplemente
queda fuera de la tabla de métodos — no se rechaza el programa completo por
eso. Solo si el programa **de verdad llama** a ese método, `gen_call` falla
con un mensaje que nombra el record y el método. Este diseño evita el
problema que tenía la primera versión (sección 61): rechazar cualquier
`impl` presente en el archivo, aunque nada lo usara, habría hecho fallar
programas perfectamente compilables solo porque declaraban de más.

También se resolvió un problema de orden que no había aparecido todavía:
Ostrin permite llamar a una función declarada más abajo en el archivo (o que
dos métodos se llamen mutuamente), pero C exige una declaración previa. Se
solucionó emitiendo **prototipos** de todas las funciones y métodos antes de
cualquier cuerpo — nunca hizo falta ordenar el grafo de llamadas.

### Pruebas

`examples/native_methods.ostrin` combina un `Counter` con un método
`mut self` que muta un campo compartido por identidad (`counter.increment`
llamado dos veces sobre el mismo puntero) y un `Point` cuyo método llama a
una función libre declarada más abajo en el archivo — ambos casos ejercitan
directamente las dos piezas nuevas (mutación a través de `self` y
prototipos de reordenamiento). Se compiló y ejecutó de verdad, comparando
contra la salida ya verificada del intérprete. Suite del compilador:
**80 pruebas**, sin warnings nuevos.

Frontera actual del backend nativo: funciones, records y sus métodos no
genéricos (inherentes o de trait, da igual — ninguno necesita dispatch),
sobre `Int`/`Float`/`Bool`/`String`. Lo único que de verdad falta para que
esta frontera se vuelva un proyecto distinto (no una extensión más) es todo
lo que exige resolver algo en tiempo de ejecución: `dyn Trait`, genéricos
reales, y operadores sobre tipos de usuario (que si necesitan resolver cuál
`impl` aplica, a diferencia de una llamada nombrada `.metodo()`).

---

## 64. `enum` y `match` en el backend nativo — 2026-09-17

Antes de empezar se le puso al usuario una limitación real sobre la mesa: al
no soportar genéricos, `Option<T>`/`Result<T, E>` — el enum que aparece en
casi todo programa real de Ostrin — quedarían fuera de todos modos. Aun así
se pidió seguir, porque un `enum` de usuario sin genéricos sigue siendo un
caso real (máquinas de estado simples, por ejemplo).

### Por qué tampoco esto necesitó dispatch dinámico

`match` se compila a una unión etiquetada (`struct { int tag; union {...}
data; }`) y una secuencia de `if` sobre `tag` — es exactamente lo que
`switch` haría, sin ninguna tabla de despacho: el compilador de C ya sabe,
en cada `if`, exactamente qué comparar. La diferencia con `record` (sección
62) es la representación: un enum se pasa **por valor**, nunca por puntero,
porque `Value::EnumInstance` en el intérprete se clona profundamente (su
`HashMap` interno se copia) cada vez que el `Value` que lo contiene se
clona — es decir, ya se comporta como un tipo de valor en el intérprete, no
uno con identidad compartida como `Record`. Copiar por valor en C es
simplemente lo que le corresponde a ese mismo comportamiento.

### Dos bugs reales, encontrados probando contra un programa real

1. **Campos posicionales en patrones.** `Circle(radius)` funciona porque
   `radius` es tanto el nombre del campo declarado como el nombre de binding
   elegido. Pero `Rectangle(Int, Int)` no tiene nombres de campo — son
   posicionales — y el patrón `Rectangle(width, height)` usa nombres de
   binding que **no** coinciden con ningún campo declarado. El propio
   intérprete resuelve esto con una regla de repliegue: primero busca por
   nombre, y si no hay campo con ese nombre, cae a la posición
   (`pattern_field_value` en `interpreter/mod.rs`). El generador de C
   replicó exactamente esa misma regla — no inventó una propia.
2. **Guardas que leen su propio binding.** `n if n > 0 => ...` (o, en el
   programa de prueba, `big if big < 0 => ...`) necesita que `big` ya exista
   como variable de C **antes** de evaluar la condición de la guarda. La
   primera versión metía la guarda dentro de la misma condición `if` que
   decide si el patrón calza, así que `big` se usaba antes de declararse —
   un error de compilación de C real, no silencioso, pero real al fin.
   Corregido separando la estructura en dos niveles: el `if` exterior solo
   comprueba lo estructural (tag/literal/rango) y declara los bindings; un
   `if` **anidado**, después de esas declaraciones, evalúa la guarda. Caer
   por una guarda que da `false` dentro de ese `if` interior deja el
   programa exactamente donde debía: sin marcar "matched", listo para que un
   brazo posterior (incluso con el mismo patrón y otra guarda) se pruebe.
3. (Menor, encontrado en la primera compilación) **Argumentos nombrados en
   constructores de variantes.** `Circle(radius: 3)` es la forma idiomática
   de construir una variante con campos — a diferencia de una llamada de
   función normal, donde este backend sigue rechazando argumentos nombrados
   sin más. Se le dio soporte específico solo para construcción de variantes
   (resueltos por nombre de campo cuando son nombrados, por posición cuando
   no).

### Qué queda fuera, explícitamente

`print()` sobre un valor de enum falla con un error claro — mostrarlo bien
necesitaría generar una función de formato por enum que decida el `printf`
correcto según el `tag`, y no se hizo hoy. Patrones anidados dentro de los
campos de una variante (`Some(Some(x))`) también fallan explícitamente. Un
enum que se contiene a sí mismo por valor (recursivo sin indirección) o dos
enums que se referencian mutuamente en un orden desfavorable producirán un
error de compilación de C (no un error de `ostrinc`) — un hueco conocido,
no detectado en esta capa, documentado aquí en vez de en el código porque es
un caso extremadamente raro comparado con un enum conteniendo un record por
puntero (que sí funciona sin importar el orden).

### Pruebas

`examples/native_enums.ostrin` combina una variante con campo nombrado
(`Circle(radius: Int)`, construida con argumento nombrado), una variante
posicional (`Rectangle(Int, Int)`, destructurada por posición), una
variante unitaria, y un `match` sobre un `Int` plano que ejercita un
literal, un rango, una guarda que lee su propio binding, y un comodín. Se
compiló y ejecutó de verdad, comparando contra la salida ya verificada del
intérprete. Se actualizó también `native_backend_rejects_constructs_it_does_not_support_yet`
(que usaba `shapes.ostrin` para probar el rechazo de `enum`): ahora ese
mismo archivo sigue fallando, pero por su campo `Quantity<Length>`, no por
el `enum` en sí — la razón del rechazo cambió porque el alcance del backend
cambió de verdad. Suite del compilador: **81 pruebas**, sin warnings nuevos.

Frontera actual del backend nativo: funciones, records (con métodos) y
enums (con match) no genéricos, sobre `Int`/`Float`/`Bool`/`String`. Lo que
falta para dejar de ser una serie de extensiones incrementales y convertirse
en un proyecto distinto sigue siendo lo mismo de siempre: todo lo que
requiere resolver algo en tiempo de ejecución en vez de en tiempo de
compilación — `dyn Trait`, genéricos reales (que es lo único que separa a
este backend de soportar `Option`/`Result`, el enum más usado del lenguaje).

---

## 65. Genéricos reales (monomorfización) en el backend nativo — 2026-09-17

Se le preguntó al usuario cómo seguir tras cerrar el subconjunto "sin
dispatch dinámico" (funciones, records+métodos, enums+match); eligió
genéricos reales vía monomorfización — el mismo mecanismo que usan las
plantillas de C++ o los genéricos de Rust: generar una función de C
distinta por cada combinación concreta de tipos con la que se llama una
función genérica, en vez de una única función genérica en tiempo de
ejecución.

### Por qué "monomorfización" no es lo mismo que "genéricos reales" del todo

Se acotó desde el principio, explícitamente ante el usuario: `Option<T>` y
`Result<T, E>` son genéricos, así que quedan fuera de este alcance de todas
formas — lo que se ganó hoy es **funciones** genéricas definidas por el
usuario, no el enum genérico más usado del lenguaje. Records y enums
genéricos, y métodos genéricos dentro de un `impl`, siguen fuera (se
descartan silenciosamente de sus tablas respectivas, como ya pasaba con
otros casos no soportados).

### Cómo funciona

- Una función `fn identity<T>(x: T) -> T` se guarda aparte
  (`Codegen.generic_functions`), nunca se le asigna una única firma de C
  (no tiene una, tiene una por cada instanciación) y nunca se emite por sí
  misma.
- En cada sitio de llamada, `infer_generic_substitutions` deduce `T` a
  partir de los tipos **concretos** de los argumentos — nunca del tipo de
  retorno ni del contexto donde se usa el resultado, a diferencia de
  `typeck`, que sí hace inferencia bidireccional completa. Si `T` solo
  aparece en el tipo de retorno, este backend no puede inferirlo y falla
  con un mensaje claro (`typeck` normalmente ya habría rechazado ese mismo
  programa con su propio error, así que en la práctica este caso casi
  nunca llega vivo hasta aquí — pero el generador no confía en eso, falla
  por su cuenta si ocurre).
- El nombre mangled (`identity__Int`, `pair__Int_String`) sirve de clave
  de caché (`Codegen.instantiations`): llamar `identity(3)` dos veces
  reutiliza la misma función de C, no la duplica.
- Como una instanciación solo se descubre mientras se genera el CUERPO de
  otra función (nunca antes), y C exige que el prototipo exista antes de
  cualquier uso — incluso si ese uso está en una función escrita más arriba
  en el archivo generado — hubo que invertir el orden de generación: ahora
  **todos los cuerpos** (funciones planas, métodos, e instanciaciones
  genéricas, vaciando una cola de pendientes hasta que quede vacía — una
  instanciación puede a su vez disparar otra) se generan primero en
  memoria, y solo cuando la cola está vacía y todas las firmas son
  conocidas se escriben los prototipos y después los cuerpos ya generados.
  Antes de este cambio, prototipos y cuerpos de funciones normales se
  escribían intercalados directamente en el archivo de salida.
- `self`/`Self` en métodos y `<T>` en funciones genéricas terminaron siendo
  el mismo mecanismo: ambos son solo un mapa de sustitución nombre→tipo
  concreto (`map_type_with_subst`), aplicado antes de cualquier otra regla
  de resolución de tipos. `map_method_type` (una función aparte para
  `Self`) desapareció, reemplazada por esta versión única.
- Un método/función genérica que dentro de su cuerpo llama a un método
  sobre su parámetro `T` (p. ej. `T: Ord` y `a.compare(b)`) simplemente
  funciona sin código adicional: en el punto de generación del cuerpo `T`
  ya se sustituyó por un record concreto, y la búsqueda de métodos ya
  existente (sección 63) encuentra el método real de ese record. Ninguna
  infraestructura de despacho nueva hizo falta para esto tampoco.

### Pruebas

`examples/native_generics.ostrin` llama `identity<T>` con `Int`, `String` y
un record (`Pair`), y `max<T>` con `Int` comparando mediante `if`. Un test
compila y ejecuta el binario real comparando contra la salida ya verificada
del intérprete; otro inspecciona el C generado con `--emit-c` y confirma
exactamente una definición de C por cada combinación (función, tipo)
realmente usada — ninguna duplicada, ninguna síntesis genérica filtrada al
código (`<T>` no debe aparecer en ningún lado). Suite del compilador:
**83 pruebas**, sin warnings nuevos.

Frontera actual del backend nativo: funciones (incluidas las genéricas,
monomorfizadas), records con métodos, y enums con match — todos no
genéricos salvo las funciones. Lo único que de verdad falta para soportar
`Option`/`Result` (records/enums genéricos) o `dyn Trait` es resolver algo
en tiempo de ejecución de una forma que la monomorfización, por diseño, no
cubre: un `dyn Trait` no sabe su tipo concreto en tiempo de compilación, así
que necesitaría una vtable real — el primer mecanismo de despacho dinámico
que este backend tendría que construir desde cero.

---

## 66. `dyn Trait` real (vtables) en el backend nativo — 2026-09-17

Antes de empezar se descubrió algo que cambiaba el plan: el propio ejemplo
del proyecto (`dyn_trait.ostrin`) usa `dyn Shape` casi siempre metido dentro
de `List<dyn Shape>`, con `.fold()` y una lambda — nada de eso existe en el
backend nativo (colecciones y closures nunca se implementaron ahí). Se le
puso esto al usuario antes de tocar código: cubrir `dyn Trait` suelto
(variables/parámetros/retornos, sin listas) es factible ahora; cubrir el
caso idiomático real es un proyecto bastante más grande todavía, porque
depende de construir `List` primero. Se eligió lo primero.

### El único lugar de todo el backend donde algo se resuelve en tiempo de ejecución

Todo lo anterior — records, métodos, enums, genéricos — se resolvía
enteramente en tiempo de compilación, sin excepción. `dyn Trait` es
estructuralmente distinto: su tipo concreto está borrado a propósito, así
que no hay forma de evitar una tabla de punteros a función real. Se
representa como un puntero gordo:

```c
typedef struct { RetTy (*metodo)(void*, Args...); ... } Trait_VTable;
typedef struct { void* self; const Trait_VTable* vtable; } Trait_Dyn;
```

Para cada record que implementa el trait, se genera una "thunk" (una
función puente que solo hace el cast de `void*` al tipo concreto y llama al
método real ya compilado) y una instancia estática de la vtable apuntando a
esas thunks. Convertir un record concreto a `dyn Trait` ("boxing") es
literalmente construir ese struct: `{ .self = (void*)puntero, .vtable =
&Trait__Record__vtable }`.

### "Object safety" gratis, sin escribirla como regla aparte

Un método de trait es válido para despacho dinámico solo si `Self` no
aparece en ningún lado salvo como el receptor `self` exacto (la misma regla
que usa Rust para decidir si un trait es "object safe"). No hizo falta
escribir esa comprobación como código separado: la tabla de firmas
abstractas del trait se construye llamando a `map_type` (no
`map_type_with_subst`) sobre cada parámetro — como `map_type` no sabe nada
de `Self`, cualquier método cuya firma mencione `Self` en otro lugar que no
sea el receptor simplemente **falla al mapearse** y se descarta de la
tabla, en vez de intentar despachar algo que no tendría sentido (dos
instancias de tipos concretos distintos combinadas a través de un
`Self` compartido).

### Igual que con los genéricos: se descubre bajo demanda, se cachea

Igual que una instanciación genérica, una vtable de (trait, record) solo se
genera la primera vez que `coerce()` necesita convertir ese record
concreto a ese trait — no para cada combinación posible de antemano. Un
`HashSet` de pares ya vistos evita duplicar la vtable si el mismo record se
convierte al mismo trait dos veces en el programa.

### El bug real que salió al probarlo

Las vtables se generaban correctamente, pero al principio se emitían
**después** de todos los cuerpos de función — igual que se había hecho con
las instanciaciones genéricas. El problema: una instancia de vtable
(`static const Trait_VTable Trait__Record__vtable = {...};`) no es solo una
declaración de función (que puede ir después, con un prototipo antes) — es
una definición completa de una variable, y C no tiene forma de
"prometerla" antes con un prototipo. Como `ostrin_main` suele ser el primer
lugar donde algo se convierte a `dyn Trait`, su cuerpo (emitido antes)
terminaba usando una vtable que aún no existía en el archivo — error de
compilación de C real. Se corrigió moviendo la emisión de las instancias de
vtable a justo después de los prototipos de las thunks (que sí pueden
preceder su propio cuerpo), y antes de cualquier cuerpo de función.

### Pruebas

`examples/native_dyn_trait.ostrin`: un trait con dos métodos, dos records
que lo implementan, una función que recibe `dyn Shape` como parámetro
(cajeando dos records distintos en dos llamadas), y un binding con tipo
explícito `dyn Shape`. Se compiló y ejecutó de verdad, comparando contra la
salida ya verificada del intérprete. Un segundo test confirma que cajear el
mismo record al mismo trait dos veces no duplica su vtable. Suite del
compilador: **85 pruebas**, sin warnings nuevos.

Frontera actual del backend nativo: funciones (incluidas genéricas),
records con métodos, enums con match, y valores `dyn Trait` sueltos — todo
resuelto en tiempo de compilación excepto la única llamada a través de una
vtable. Lo que sigue, si alguna vez se quiere cerrar la brecha real de
`dyn_trait.ostrin`, es construir `List`/colecciones — un proyecto aparte,
no una extensión de lo que ya existe.

---

## 67. `List<T>` en el backend nativo — 2026-09-17

Se continuó exactamente por donde la sección 66 dejó marcado el siguiente
paso.

### Misma identidad de referencia que un record, misma monomorfización que una función genérica

`Value::List` en el intérprete es `Rc<RefCell<Vec<Value>>>` — identidad
compartida, igual que `Record` (dos bindings que comparten una lista deben
ver el `push` del otro). Por eso `List<T>` se representa exactamente igual
que un record: siempre por puntero, nunca por valor, a un struct reservado
con `malloc` que crece con `realloc` cuando hace falta más capacidad.

Pero el TIPO `List<T>` en sí mismo es genérico, así que su instanciación
sigue el mismo patrón que una función genérica (sección 65): la primera vez
que se ve una `List` de un tipo de elemento concreto, se genera su struct
(`List_Int`, `List_Circle`, ...) y cinco funciones de ayuda
(`_new_from_array`, `_push`, `_length`, `_get` con comprobación de límites,
`_remove_at` con desplazamiento de elementos) — nunca antes de que algo la
use de verdad, y reutilizadas si el mismo tipo de elemento vuelve a
aparecer.

### Un caso que ninguna de las llamadas ya existentes cubría

Todo el resto de instanciaciones perezosas (genéricos, vtables) se
descubrían siempre desde dentro de un CUERPO de función, porque solo ahí se
generaban expresiones reales. Pero una `List<T>` puede aparecer **solo en
una firma** — un parámetro `numbers: List<Int>` cuya función nunca
construye una lista nueva, solo reenvía la que le pasaron — y las firmas se
resuelven con `map_type`, una función libre sin `&mut self` que no puede
encolar nada. Se resolvió con `register_list_types`, una pasada aparte que
recorre cualquier `CType` ya resuelto (de un parámetro, un retorno, un
campo de record o de variante) buscando cualquier `List` anidada y
encolándola — sin este paso, `sum_list(numbers: List<Int>) -> Int` en el
programa de prueba habría compilado el propio `sum_list` sin que
`List_Int` existiera todavía en el archivo generado.

### El mismo problema de orden que ya había aparecido con las vtables, en una forma nueva

El struct de una lista (a diferencia de un record o un enum, cuyos structs
se conocen de antemano por las declaraciones del programa) solo se conoce
completo **después** de la fase de generación de cuerpos — la misma cola de
pendientes que ya drenaban genéricos y vtables. Eso significa que el struct
de `List_Int` no puede ir junto a los de `record`/`enum` al principio del
archivo (donde se emitían antes de saber qué listas existen); tiene que
emitirse justo después de que esa cola termine de vaciarse, y antes de
cualquier prototipo de función — un tercer punto de inserción distinto a
los dos que ya había para genéricos/vtables, pero siguiendo la misma idea:
nada se escribe al archivo final hasta que todo lo que puede descubrirse
por uso ya se descubrió.

### Pruebas

`examples/native_lists.ostrin` ejercita tres instanciaciones a la vez
(`List<Int>`, `List<Point>`, `List<String>`): literales, `.length()`,
`.push()`, indexado, `.remove_at()`, `for x in lista`, y una función
(`sum_list`) cuyo único punto de contacto con `List<Int>` es su firma —
exactamente el caso que `register_list_types` existe para cubrir. Se
compiló y ejecutó de verdad, comparando contra la salida ya verificada del
intérprete. Un segundo test confirma que cada tipo de elemento genera
exactamente un struct, no uno por cada literal/uso. Suite del compilador:
**87 pruebas**, sin warnings nuevos.

Se verificó además, sin que fuera parte del alcance pedido, cuánto se
acercó esto al ejemplo real `dyn_trait.ostrin` (`List<dyn Shape>` +
`.fold()` + una lambda): con `List` ya soportado, ese archivo avanza mucho
más lejos y ahora falla específicamente en la lambda pasada a `.fold()` —
confirma que el único bloqueo que queda para ese ejemplo concreto son los
closures, no las listas.

Frontera actual del backend nativo: funciones (incluidas genéricas),
records con métodos, enums con match, `dyn Trait` sueltos, y `List<T>` con
sus operaciones básicas (`length`/`push`/`remove_at`/indexado/`for`).
`map`/`filter`/`fold`/`find`/`any`/`all` — y con ellos, el `dyn_trait.ostrin`
original — siguen bloqueados por lo mismo: closures, la única pieza de
"resolver algo en tiempo de ejecución" que este backend todavía no tiene
ninguna forma de representar.

---

## 68. Lambdas en combinadores de `List` (backend nativo) — 2026-09-18

Cierra el ejemplo original `dyn_trait.ostrin` (`List<dyn Shape>` + `.fold()`
con lambda): ahora compila a nativo y su salida coincide dígito a dígito con
el intérprete.

### Diseño: sin objetos closure

En vez de funciones anónimas con entorno capturado (punteros a función +
structs de entorno + análisis de escape), la lambda solo se acepta como
argumento **directo** de `map`/`filter`/`fold`/`any`/`all` y el combinador se
**expande en línea** como un bucle dentro de una expresión-sentencia GNU. El
cuerpo de la lambda queda en el mismo ámbito C que el código que la rodea, así
que las variables capturadas funcionan sin ningún mecanismo extra. El tipo de
los parámetros no se declara en Ostrin: viene del combinador (elemento de la
lista; en `fold`, el tipo del valor inicial), y el tipo del resultado sale del
cuerpo (`map` puede cambiar el tipo de elemento; `fold` coacciona el cuerpo al
tipo del acumulador, lo que permite acumular sobre valores `dyn`).

### Otros cambios
- `count()` como alias de `length()`.
- Un literal de lista ligado a `List<dyn Trait>` embolsa cada elemento
  (`gen_list_literal` recibe el tipo de elemento esperado).
- `print` de `Float` usa `ostrin_print_float`: busca la menor precisión que
  hace ida y vuelta exacta (como el formato por defecto de Rust), en vez de
  `%g` (6 cifras), que daba 12.5664 frente a 12.56636 del intérprete.

### Límites
Valores función de primera clase (guardar una lambda en una variable o pasarla
a una función de usuario) y `find` (devuelve `Option<T>`, genérico) siguen
solo en el intérprete. Suite: **89 pruebas**, sin warnings.

---

## 69. `Option<T>` en el backend nativo — 2026-09-18

`Option` no es un `enum` declarado en el AST (el intérprete lo registra como
incorporado), así que no podía pasar por el camino de enums de usuario. Se
representa como `{ bool has; T value; }` por valor, monomorfizado por `T`
(`Codegen::ensure_option`, misma cola perezosa que listas/genéricos).

- `Some(x)` infiere `T` del argumento. Un `None` desnudo no lleva `T`: tiene el
  tipo interno `CType::NoneLit` y `coerce` lo convierte a `Option<T>` cuando
  encuentra el tipo esperado (retorno, binding con tipo, argumento, o el otro
  brazo de un `if`/`match`). En `match` se difiere la coerción de cada brazo
  hasta conocer el primer tipo que no sea `None`.
- Patrones `Some(v)` / `Some(_)` / `None`; métodos `is_some`, `is_none`,
  `unwrap`, `unwrap_or`; y `List.find(lambda)`, que era lo que bloqueaba los
  combinadores.
- Arreglo de paso: un `match` cuyos brazos son todos `Void` (usado por efecto)
  declaraba una variable `void`; ahora no genera variable de resultado.

Fuera de alcance: `Result<T,E>`, `?`/`try`, `print` de un Option. Suite: **90
pruebas**, sin warnings.

---

## 70. `Result<T, E>` y `try` en el backend nativo — 2026-09-18

- `Result<T,E>` es `{ bool ok; T value; E error; }` por valor, monomorfizado por
  el par (T, E). `Ok(x)`/`Err(e)` solo conocen un lado, así que llevan tipos
  parciales (`OkLit(T)`/`ErrLit(E)`), igual que `None` (`NoneLit`); `coerce` los
  completa contra el tipo esperado, y `unify_types` los combina entre brazos de
  `if`/`match` (`Ok(x)` + `Err(e)` -> `Result<T,E>`).
- `try expr` (Ostrin no tiene `?` postfijo) se compila a un `return` de C dentro
  de una expresión-sentencia GNU: desenvuelve `Some`/`Ok` o retorna `None`/`Err`
  de la función envolvente. Con `catch fn(e) {..}` el manejador se expande en
  línea para mapear el tipo de error. Dentro de una lambda se rechaza (en el
  intérprete retornaría de la lambda; aquí retornaría de la función).
- `return expr` ahora también coacciona al tipo de retorno (antes solo lo hacía
  la expresión final, así que `return Err("x")` no compilaba).
- Patrones `Ok(v)`/`Err(e)`; métodos `is_ok`/`is_err`/`unwrap`/`unwrap_or`.

### Dos bugs previos encontrados al probarlo (arreglados)
1. **Parser:** una llamada seguida de `{` en cabecera de `match`/`if`/`while`
   (`match run(7) { ... }`) se leía como cierre final (`f(x) { y -> .. }`) y
   fallaba con "expected an expression". Ahora se desactiva en contextos sin
   literales de struct.
2. **Intérprete:** `print(f(try g()).unwrap())` desbordaba la pila (1 MiB en
   Windows, marcos grandes en debug). Todo `ostrinc` corre ahora en un hilo con
   pila de 512 MiB.

Suite: **91 pruebas**, sin warnings.

---

## 71. `Int / Int` es división entera — 2026-09-18

Al preparar `Quantity` para el backend nativo apareció un bug de solidez: el
verificador de tipos infería `Int / Int` como `Int`, pero el intérprete
devolvía `Float` (`7 / 2` -> `3.5`) y el backend nativo truncaba (`3`). Se le
preguntó al usuario cuál era la semántica correcta; eligió división entera
(coincide con el tipo ya inferido y con el doc 01, que solo protege de
"sorpresas de división entera" a `Quantity`). Cambios: el intérprete trunca y
da error claro ante división por cero; el backend nativo usa `ostrin_idiv`
(mismo error en vez de un SIGFPE). Test de regresión con ambos backends.

### Sobre `Quantity` en el backend nativo (no implementado aún)
El intérprete lleva la unidad como cadena en tiempo de ejecución
(`5 nm + 2 m` imprime `2000000004.9999998 nm`; `10 m / 2 s` imprime `5 m/s`).
En nativo la unidad tendría que conocerse estáticamente, pero un parámetro
`Quantity<Length>` no fija la unidad (puede llegar `m` o `km`), así que habría
que monomorfizar funciones por unidad de argumento e inferir el tipo de retorno
generando el cuerpo — un diseño propio, no una extensión menor.

---

## 72. `Quantity` en el backend nativo — 2026-09-18

Se descartó monomorfizar por unidad (sección 71) a favor de un diseño más
simple que reproduce el intérprete exactamente: **la dimensión es parte del
tipo estático, la unidad es una cadena en tiempo de ejecución.**

- `CType::Quantity(Dimension)`; en C, `Qty { double v; const char* u; }`.
  Una función con `Quantity<Length>` recibe cualquier unidad de longitud sin
  duplicarse, y `5 nm + 2 m` convierte en ejecución como `convert()` del
  intérprete.
- `qty_runtime.c` (`include_str!`, solo se inserta si el programa usa `Qty`)
  porta la tabla de unidades, `resolve_unit_factor` y `convert`, y los
  operadores. Las unidades compuestas se construyen en ejecución (`m/s`,
  `kg*m/s*m/s`, `1/s`).
- El tipo del resultado se decide en compilación: `Q/Q` con dimensión
  resultante vacía devuelve `Float` (`ostrin_qty_ratio`); si no, `Quantity`.
- Genéricos `<D: Dimension>`: `D` se infiere de la dimensión del argumento y
  viaja por el mismo mapa de sustitución que cualquier parámetro de tipo.
- También `as`, `within`, `approximately`, negación y escalar*Quantity. Se
  reproducen incluso rarezas del intérprete (`1500 m as km` solo etiqueta, no
  convierte; `within` compara valores crudos).
- Formato de floats: `ostrin_fmt_double` nunca usa notación exponencial
  (`400000000`, no `4e+08`), como Rust.

Con esto `physics.ostrin` compila y coincide. `shapes.ostrin` avanza mucho más
y ahora se detiene en métodos sobre **enums** (`impl Shape`), que el backend
solo resuelve en records y `dyn Trait`. Suite: **93 pruebas**, sin warnings.

---

## 73. Métodos sobre enums y `to_string()` en el backend nativo — 2026-09-18

- La tabla de métodos ya no asume record: `MethodInfo.self_ty` es un `CType`
  (`Record` o `Enum`), y `Self` se sustituye por él. Un receptor enum se pasa
  **por valor** (los enums son tipos valor), un record por puntero; el resto del
  despacho estático es idéntico.
- `.to_string()` sobre `Int`/`Float`/`Bool`/`String`/`Quantity` (helpers en C;
  mismo formato de float que `print`). Records/enums no (necesitarían un Display
  generado).
- Con esto `shapes.ostrin` (enum + `impl` + lista de enums + `Quantity` +
  `to_string`) compila y coincide con el intérprete.

### Estado: barrido de los ejemplos con `--compile`
Los que aún fallan en nativo por falta de soporte (no por errores de tipo):
records/enums **genéricos** (`Box<T>`, `Maybe<T>`), operadores por `derive`
(`==`/`<` sobre records), **argumentos nombrados** en llamadas normales, `Map`/
`Set`, canales/`spawn`, iteradores propios (`impl Iterator`), inferencia de un
`T` que solo aparece en el retorno, e `imprimir` de una `List`. Suite: **93
pruebas**, sin warnings.

---

## 74. Records y enums genéricos en el backend nativo — 2026-09-18

- Un record/enum genérico no tiene tipo C propio: cada instanciación concreta
  (`Score<Int>`, `Maybe<Outcome<Int, String>>`) se **monomorfiza** a un tipo con
  nombre mangleado (`Score__Int`) que se registra bajo demanda. `map_type` (sin
  `&mut self`) las anota en un `RefCell`; `flush_instances` las registra (campos,
  variantes y los métodos de todo `impl` aplicable: `impl<T> Box<T>` genérico o
  `impl Trait for Box<Int>` especializado) y se invoca tras cada expresión.
- `map_type` ahora sustituye parámetros de forma recursiva, así que
  `List<T>`, `Option<T>`, `Result<T,E>` y `Pair<A,B>` funcionan dentro de
  funciones genéricas; la inferencia de `T` es estructural (`bind_type`).
- Inferencia bidireccional mínima: `Codegen.expected` lleva el tipo esperado
  (argumentos de funciones/métodos, campos, `return`/cola, anotaciones,
  asignaciones) para completar `Just(Good(3))`, `Pair { .. }` y un `Nothing`
  suelto (`CType::GenLit`, como `NoneLit` para Option). Se admiten tipos
  explícitos: `Item<Int>(1)`, `Score<Int> { .. }`, `identity<Int>(7)`.
- Patrones: variantes anidadas (`Just(Good(n))`), destructuración de records
  (`Pair(first: v, second: _)`) y variantes sin campos escritas como
  `Pattern::Ident`.
- Orden de emisión en C: typedefs de todo, cuerpos de enums, cuerpos de
  records; los `List_*` se declaran (typedef) antes de los records.
- Sigue fuera: métodos genéricos (`fn map<U>`), `print` de un enum, y el resto
  de la lista del apartado 73. Suite: **93 pruebas**, sin warnings.

---

## 75. `print` de records y enums en el backend nativo — 2026-09-18

- `print` de un record/enum (incluidas instancias genéricas y valores anidados)
  usa un `ostrin_show_<Nombre>` generado bajo demanda, con el mismo formato que
  el intérprete: `Circle(radius: 1.5)`, `Rect(2, 3)`, `Dot`, `P { x: 1, name: a }`.
- Sigue sin poder imprimirse `List`/`Option`/`Result`. Ejemplo nuevo
  `native_display.ostrin`. Suite: **93 pruebas**, sin warnings.

---

## 76. Operadores sobre records y enums en el backend nativo — 2026-09-18

- Mismo orden que `eval_binary` del intérprete: primero el método del usuario
  (`impl Add/Sub/Mul/Div` → `add`…, `impl Eq` → `equals`, `impl Ord` → `compare`),
  luego `derive(Eq)` (estructural, campo a campo; enums: variante y campos) y
  `derive(Ord)` (lexicográfico, solo records). Se generan `ostrin_eq_*`/
  `ostrin_cmp_*` bajo demanda; `!=` niega `==`.
- `Ordering` (`Less/Equal/Greater`) es ahora un enum real del backend, para que
  un `compare` escrito a mano compile. `traits.ostrin` compila y coincide.
- Ejemplo nuevo `native_derive.ostrin`; el test que exigía rechazar operadores
  sobre records se eliminó. Sigue sin soporte: `==` sobre `List`/`Option`.

---

## 77. Argumentos nombrados y por defecto en el backend nativo — 2026-09-18

- Las llamadas a funciones (genéricas o no) y a métodos resuelven argumentos
  nombrados y valores por defecto antes de generar código
  (`normalize_call_args`): los posicionales rellenan en orden, los nombrados por
  parámetro y los que faltan toman su `default`, evaluado en el sitio de la
  llamada como en el intérprete.
- `function_arguments.ostrin` compila y coincide; ejemplo nuevo
  `native_named_args.ostrin`. (El checker no admite defaults en métodos, solo
  nombrados.) Suite: **93 pruebas**, sin warnings.

---

## 78. `Map`, `Set` y `print` de colecciones en el backend nativo — 2026-09-18

- `Map<K, V>` y `Set<T>` son estructuras en el heap (por referencia, como
  `List`), con arrays que conservan el orden de inserción y búsqueda lineal por
  igualdad (`eq_expr`, así que sirven claves `Int`/`String`/records con
  `derive(Eq)`). Métodos: `get`/`set`/`remove`/`contains_key`/`count`/`keys`/
  `values` y `add`/`contains`/`remove`/`count`; literales `["a": 1]` y `{1, 2}`.
- El parser descartaba los tipos de `Map<K, V>()`/`Set<T>()`; ahora produce
  `Expr::EmptyCollection(nombre, tipos)` (tipado igual que antes, sin cambios de
  comportamiento en el intérprete).
- `print` de `List`, `Option`, `Map` y `Set` mediante `ostrin_show_*` generados
  (`[1, 2]`, `Some(4)`, `[a: 1]`, `{a}`). Siguen sin imprimirse los `Result`.
- `collections.ostrin` compila y coincide; el test de rechazo ahora usa
  `concurrency.ostrin` (canales). Suite: **93 pruebas**, sin warnings.

---

## 79. Métodos por defecto de traits en el backend nativo — 2026-09-18

- El cuerpo por defecto de un método de trait se convierte en una
  `FunctionDecl` y se compila una vez por cada `impl` que no lo sobrescribe
  (`impl_method_list`), tanto en tipos normales como en instancias genéricas
  (`impl Named for Box<Int>`). También quedan disponibles a través de `dyn Trait`.
- `traits_defaults*.ostrin` compilan y coinciden; ejemplo nuevo
  `native_trait_defaults.ostrin`.
- Aclaración: ni `Map` ni `Set` se pueden recorrer con `for` en el lenguaje
  (el intérprete responde «is not iterable»), así que no hay nada que portar.
- Pendiente en el barrido: canales/`spawn`, iteradores propios, métodos
  genéricos, `impl` sobre `Quantity<D>`, lambdas sobre `Option`/`Result`
  (`option_result.ostrin`), `read_file` y compañía. Suite: **93 pruebas**.

---

## 80. Combinadores de `Option`/`Result` y `print` de `Result` en el nativo — 2026-09-18

- Con lambdas expandidas en línea (como en las listas): `Option.map/then/ok_or`
  y `Result.map/map_err/then/ok`. Tipos explícitos: `None<Int>()`,
  `Ok<T, E>(x)`, `Err<T, E>(e)`.
- `print` de `Result` (`Ok(4)`, `Err(bad)`).
- Un `Ok(x)`/`Err(e)`/`None` ligado a un nombre sin anotación solo conoce la
  mitad de su tipo; la mitad desconocida se rellena con un `Int` de relleno
  (`settle_literal`), que un programa que pasó el checker nunca observa.
- `option_result*.ostrin` y `try_result.ostrin` compilan y coinciden.
  Suite: **93 pruebas**, sin warnings.

---

## 81. Métodos genéricos e `impl` sobre `Quantity` en el backend nativo — 2026-09-18

- Un método con parámetros propios (`fn map<U>(self, v: U) -> U`) se guarda
  aparte (`GenericMethod`) y se monomorfiza en cada llamada: `U` se infiere de
  los argumentos o del `<...>` explícito, y se instancia como una función más
  (`Tipo__metodo__Int`). Sirve en records, enums, instancias genéricas y
  cantidades, y desde funciones genéricas con cotas (`apply<T: Mapper, U>`).
- `impl Trait for Quantity<Length>` y `impl<D: Dimension> ... for Quantity<D>`:
  los métodos se registran perezosamente por dimensión al primer uso
  (`ensure_quantity_methods`).
- `Codegen.subst_stack` lleva la sustitución de la función en curso, de modo
  que un tipo escrito dentro de un cuerpo genérico (`map<U>`, `List<T>`, anotaciones)
  se resuelve con los tipos concretos de esa instancia (`resolve_type`).
- `generic_impls_and_methods`, `generics_explicit` y `quantity_impl_dispatch`
  compilan y coinciden; ejemplo nuevo `native_generic_methods.ostrin`.
  Suite: **93 pruebas**, sin warnings.

---

## 82. Funciones incorporadas en el backend nativo — 2026-09-18

- `read_file`, `write_file`, `parse_int` (mismos mensajes de error que Rust:
  «invalid digit found in string», «cannot parse integer from empty string»,
  «number too large…»), `sum` (Int/Float/Quantity) y `panic`.
  `Result<Void, String>` usa un `char` de relleno para su campo de valor.
- Los mensajes de error del sistema de `read_file`/`write_file` vienen de
  `strerror`, así que difieren del texto de Rust/Windows (`stdlib_io.ostrin`
  compila, pero su salida de error no es idéntica byte a byte).
- `newlines.ostrin` y `advanced.ostrin` no son programas ejecutables (referencian
  nombres inexistentes: son muestras de sintaxis), así que no se portan.
- Ejemplo nuevo `native_builtins.ostrin`. Suite: **93 pruebas**, sin warnings.

---

## 83. Iteradores propios en el backend nativo — 2026-09-18

- `for x in registro` donde el record tiene `next(mut self) -> Option<T>` se
  compila a un bucle que llama a `next` hasta recibir `None` (protocolo del
  intérprete). `fibonacci.ostrin` compila y coincide.
- Queda fuera solo la concurrencia (canales/`spawn`). Suite: **93 pruebas**.

---

## 84. Canales, `spawn` y `join` en el backend nativo — 2026-09-18

- Mismo modelo que el intérprete: `spawn { .. }` se ejecuta de inmediato y de
  forma síncrona; su resultado queda en un `Task_T` (por valor) que `join()`
  devuelve. `channel<T>()` es una cola FIFO en el heap con `send`, `receive`
  (→ `Option<T>`), `close` y `for x in canal` (consume hasta vaciar).
  No hay hilos ni bloqueos, así que la salida coincide con el intérprete.
- Enviar un **record** por un canal se rechaza en la compilación: el
  intérprete comprueba en ejecución que un record enviado no se reutilice
  (E1101, `moved_after_send.ostrin`) y el nativo no tiene ese análisis; se
  prefiere un error claro a ejecutar un programa que el intérprete rechaza.
- `concurrency.ostrin` y el ejemplo nuevo `native_concurrency.ostrin` coinciden.
  Con esto el barrido de ejemplos ejecutables compila entero salvo lo anterior.
  Suite: **93 pruebas**, sin warnings.

---

## 85. Etapa 0 de la arquitectura: red de seguridad — 2026-09-18

Primer paso del plan de `docs/ARQUITECTURA_Y_VISION.md`: antes de tocar la
arquitectura, pruebas que detecten regresiones entre fases.

- `compiler/tests/differential.rs` (nuevo, 3 pruebas):
  1. **Diferencial intérprete↔nativo** sobre *todos* los `examples/*.ostrin`
     (los nuevos entran solos). Solo se toleran listas explícitas y razonadas:
     `KNOWN_NATIVE_GAPS` (debe fallar al compilar; si un hueco se cierra, la
     prueba obliga a borrar la entrada), `KNOWN_OUTPUT_DIFFERENCES` y
     `NOT_PROGRAMS`. Exige un mínimo de ejemplos comparados.
  2. **Compile-fail**: todo `*_error(s).ostrin` debe rechazarse con un código
     `OSTRIN-E….` estable.
  3. **Fuzzing por mutación** determinista (xorshift; `OSTRIN_FUZZ_ROUNDS=N`
     para más rondas) de lexer/parser/checker: ningún `panic`.
- Hallazgo real: los errores de sintaxis no tenían código. Ahora los errores de
  análisis llevan `OSTRIN-E0001` (sintaxis) y los léxicos `OSTRIN-E0002`, tanto
  en texto como en el JSON de diagnósticos (también los de módulos).
- CI (`.github/workflows/ci.yml`): matriz Ubuntu + Windows + macOS y
  `OSTRIN_REQUIRE_CC=1`, para que la falta de compilador C haga fallar la
  prueba diferencial en lugar de omitirla en silencio.
- Suite: **96 pruebas** (93 + 3), sin warnings.

---

## 86. Etapa 1: tabla de tipos por expresión — 2026-09-18

- El checker ahora conserva, además de los errores, el **`Ty` real de cada
  expresión** (`TypedProgram.expr_types`, clave `ExprKey { archivo, inicio, fin }`;
  el parser envuelve cada expresión exactamente una vez en `Expr::Located`, así
  que la clave es única sin tocar el AST). Un `Unknown` posterior nunca borra un
  tipo ya determinado (los lambdas se infieren dos veces). Es la base del futuro
  HIR: el backend podrá leer tipos en vez de reinferirlos.
- `ostrinc --typed-report archivo.ostrin`: nº de expresiones, nº con tipo
  desconocido y su ubicación. Auditoría inicial: **1 382 expresiones en los
  ejemplos válidos, 46 desconocidas (3,3 %)**.
- Arreglo encontrado por la auditoría: `Map<K,V>()`/`Set<T>()` se tipaban con
  `Unknown`; ahora usan los tipos escritos.
- Test-de trinquete (`typed_expression_table_does_not_regress`): el número de
  desconocidas no puede subir (límite 46; bajarlo al cerrar huecos). Causas que
  quedan: `None`/`Ok(x)`/`Err(e)` sin contexto esperado, elementos `dyn Trait`,
  y sus derivados.
- Decisión: no se añadió un `NodeId` al AST; la clave por rango cubre el
  objetivo con un cambio mucho menos invasivo. Se revisará si hiciera falta
  para nodos sin `Located` (patrones, declaraciones).
- Suite: **97 pruebas**, sin warnings.

---

## 87. Etapa 1 (cont.): inferencia con tipo esperado en el checker — 2026-09-18

- `Checker::note_expected`: cuando el contexto conoce el tipo esperado
  (cola de función, `return`, anotación de `let`, asignación a campo, argumentos
  de llamadas —posicionales y con nombre—), el tipo registrado de una expresión
  parcial (`None`, `Ok(x)`, `Nothing`, `Just(Good(3))`) se **refina** al esperado
  y la expectativa se propaga a ramas de `if`/`match`/bloques, listas y
  argumentos de constructores de variantes (incluidos los genéricos, con la
  sustitución de parámetros). Es aditivo: no emite errores ni reemplaza tipos ya
  conocidos, por lo que el comportamiento del checker no cambia.
- Nuevo `Ty::Dyn(trait)`: antes `dyn Trait` se resolvía a `Unknown`, es decir,
  **ningún valor `dyn` estaba tipado**. Ahora `dyn Shape` es un tipo y las
  llamadas a sus métodos devuelven el tipo declarado en el trait. La
  compatibilidad sigue siendo permisiva (un tipo concreto se acepta donde se
  espera `dyn`); endurecerla (comprobar que implementa el trait) queda pendiente.
- Desconocidas en ejemplos válidos: **46 → 11** de 1 382. Las que quedan son
  las muestras de sintaxis (`advanced`, `newlines`), un `Ok(7)` sin anotación ni
  contexto (genuinamente indeterminado) y un `try … catch`. Límite del test de
  trinquete: 11.
- Suite: **97 pruebas**, sin warnings.

---

## 88. Etapa 2 (arranque): detector de divergencias checker↔nativo — 2026-09-18

- `codegen::generate_with_report(items, &tabla_de_tipos)` genera **exactamente
  el mismo C**, pero además compara, expresión por expresión, el tipo que infiere
  el backend con el de la tabla del checker (`NativeTypeReport`: `agreed`,
  `partial`, `unchecked`, `divergences`). CLI: `ostrinc --native-type-report`.
  Se compara solo en código no genérico (las instancias monomorfizadas tienen
  tipos abstractos en el checker).
- Medición inicial sobre los ejemplos compilables: **1 260 de ~1 350 expresiones
  coinciden**, 16 son literales parciales del backend (`None`, `Ok(x)`, …) que el
  checker ya conoce completos (candidatas a retirar la reinferencia), 73 no
  comparables (instancias genéricas o sin tipo) y **1 divergencia real**.
- La divergencia era un **bug del checker**: en `for n in fib` sobre un record con
  `impl Iterator<Int>`, `n` se tipaba como `Fibonacci` en vez de `Int`. Corregido
  (el elemento es el argumento de `Iterator<T>`). Primer fruto del detector.
- Test `native_backend_types_agree_with_the_checker`: cero divergencias en todos
  los ejemplos. Sirve de red durante la migración del backend al HIR.
- Suite: **98 pruebas**, sin warnings.

---

## 89. Etapa 2: el backend empieza a leer los tipos del checker — 2026-09-18

- `codegen::generate_with_report` es ahora la única entrada del generador y
  `--emit-c`/`--compile` le pasan la tabla de tipos del checker
  (`check_program_typed`). Primera porción retirada: los **literales parciales**
  (`None`, `Ok(x)`, `Err(e)`, un `Nothing` suelto). Antes esperaban a que un padre
  aportase una pista (`expected`/`coerce`); ahora, en código no genérico, se
  completan en el propio nodo con el tipo del checker (`complete_from_checker`,
  con `ty_to_ctype` como inverso de `ctype_agrees`).
- Resultado: los 16 literales parciales de los ejemplos se completan
  (`partial == completed`, comprobado por test). Las pistas antiguas siguen
  activas como respaldo (instancias genéricas, `Ok(7)` sin contexto), de modo que
  la salida no cambia: 5 pruebas diferenciales y las 93 de integración en verde.
- Siguiente: instancias genéricas (el checker debe dar tipos sustituibles por el
  monomorfizador) y retirar `expected`/`settle_literal` cuando ya no hagan falta.
- Suite: **98 pruebas**, sin warnings.

---

## 90. Etapa 2: instancias genéricas comparadas y constructores guiados por el checker — 2026-09-18

- Dentro de una instanciación monomorfizada, un tipo del checker se interpreta
  con la sustitución del cuerpo: `Ty::Generic("T")` ↦ el `CType` con que se
  instanció, y `Quantity<D>` ↦ la dimensión enlazada a `D`
  (`substitute_dimension`). Con ello el detector ya **compara también el código
  genérico**: **1 330 expresiones coinciden, 0 divergencias, 4 sin tipo**
  (antes 1 260 / 1 / 73).
- `checker_hint`: para constructores (`Just(9)`, `Pair { .. }`, `Nothing`, …) el
  tipo completo del checker es ahora la pista autoritativa de los argumentos de
  tipo del record/enum genérico; la inferencia propia queda solo como respaldo.
- Nuevos límites del test: `agreed > 1300`, `unchecked <= 4`.
- Queda por retirar la reinferencia de llamadas a funciones genéricas
  (`bind_type`/`infer_generic_substitutions`) y `expected`/`settle_literal`; se
  hará cuando el checker exponga las sustituciones por llamada.
- Suite: **98 pruebas**, sin warnings.

---

## 91. Etapa 2: el checker entrega las sustituciones de cada llamada genérica — 2026-09-18

- `TypedProgram.call_substs`: por cada llamada a una función genérica, los tipos y
  dimensiones con que se instanció (`CallSubst`, clave = la expresión de la
  llamada). El checker las calculaba y las descartaba.
- El backend las usa como **fuente autoritativa** al monomorfizar
  (`checker_call_subst`, con los tipos genéricos del cuerpo actual sustituidos);
  su propia inferencia (`infer_generic_substitutions`) queda como respaldo y como
  comprobación cruzada (una discrepancia se cuenta como divergencia).
  En los ejemplos: **27 de 28** llamadas genéricas salen del checker; 0 discrepancias.
  La restante es `wrap(5).is_just()` (receptor de una llamada a método) que el
  checker no registra todavía.
- Mejora del unificador del checker: un enlace parcial (`Maybe<?>` de un
  `Nothing` suelto) cede ante uno completamente conocido (`get_or(Nothing, 3)`).
- Suite: **98 pruebas**, sin warnings.

---

## 92. Paquete grande: `--test`, aserciones, prefijo de funciones y dos especificaciones — 2026-09-18

- **`ostrinc --test archivo.ostrin`**: ejecuta cada función `test_*` sin argumentos
  (en orden de aparición, cada una en un intérprete nuevo), imprime
  `test nombre ... ok/FAILED (mensaje)` y un resumen; código de salida distinto de
  cero si alguna falla. Builtins nuevos `assert(cond)` y `assert_eq(a, b)` en
  checker, intérprete y backend nativo (`assert_eq` usa la igualdad del lenguaje:
  `equals`/`derive(Eq)`). Ejemplos `testing.ostrin` y `testing_failure.ostrin`.
- **Bug encontrado**: una función de usuario llamada `double` (o cualquier palabra
  de C o de libc) rompía el C generado. Ahora toda función de usuario se emite como
  `ostrin_fn_<nombre>`. **Limitación conocida**: variables locales y campos con
  nombre de palabra reservada de C (`default`, `switch`, `register`…) todavía no se
  renombran.
- Especificaciones (rama «semántica primero» del plan):
  `docs/design/18-modelo-de-memoria-nativo.md` (recomienda RC determinista +
  análisis de último uso + arenas, y E1101 como análisis estático de movimiento)
  y `docs/design/19-jerarquia-numerica-y-arrays.md` (enteros de ancho fijo,
  `Float32`, `Complex`, `Array<T, Shape>` con broadcasting y formas estáticas).
- Suite: **99 pruebas** (94 + 5 diferenciales), sin warnings.

---

## 93. Enteros de ancho fijo (fase 1 del documento 19) — 2026-09-18

Implementado de punta a punta (léxico → parser → checker → intérprete → nativo):

- **Tipos**: `Int8 Int16 Int32 UInt8 UInt16 UInt32 UInt64` (`Int` es `Int64`; `Int128`/`UInt128`
  quedan pendientes). En C son los tipos `stdint` correspondientes.
- **Literales**: con sufijo (`200u8`, `5i32`, `9u64`, `7i64` = `Int`); un literal sin
  sufijo mayor que `Int` que cabe en 64 bits sin signo es `UInt64`.
  **Un literal sin sufijo toma el tipo que exige el contexto** (`a: UInt8 = 200`,
  `x + 1` con `x: UInt8`, argumento, `return`, cola de función, campo de record,
  `List<UInt8> = [1, 2]`) si cabe exactamente; si no, error `OSTRIN-E1070`.
  Para que intérprete y nativo lo sepan sin reinferir, el checker publica
  `TypedProgram.literal_kinds` (clave = el nodo literal) y ambos lo consultan.
  Para ello el parser ahora envuelve cada literal entero en `Expr::Located`.
- **Sin mezcla implícita**: `UInt8 + Int32`, `UInt8 + Int` (no literal) → `E1041`;
  hay que convertir con `as`. Negar un tipo sin signo → `E1041`. (La ampliación exacta
  implícita del documento 19 §3 queda pendiente; hoy es más estricto.)
- **Conversión explícita** `x as UInt8 | Int | Float | …`, con comprobación de rango
  (Float→entero trunca hacia cero y falla si sale del rango).
- **Overflow definido**: `+ - * / -x` fallan con `integer overflow` en ambos backends
  (el nativo usa `__builtin_*_overflow`); división por cero también.
- `print`, `to_string`, igualdad y orden funcionan; el nativo genera `uint8_t`…
  y `ostrin_uint_to_string`.
- Ejemplos: `sized_ints.ostrin` (compara nativo/intérprete),
  `sized_ints_errors.ostrin` (3 códigos), `sized_ints_overflow.ostrin`
  (test `fixed_width_integer_overflow_is_an_error_in_both_backends`).
- Limitaciones conocidas (actualizado: `-128i8`, argumentos de métodos y de variantes ya se aceptan; ver `sized_ints_contexts.ostrin`):
  la adaptación de literales no cubre argumentos de métodos ni de variantes de enum
  (usar sufijo); `for` sobre rangos solo admite `Int`; `sum` sobre listas sized no
  está en el nativo; ni el resaltado de VS Code ni los tokens semánticos del LSP
  conocen aún los sufijos.
- Suite: **100 pruebas** (5 diferenciales + 95 de integración), sin warnings.

---

## 94. Ergonomía de literales y `Float32` (fase 2 del documento 19) — 2026-09-18

- Cierre de los límites del §93: la adaptación de literales cubre **argumentos de
  métodos y de variantes de enum**; `-128i8` se acepta (checker, intérprete y
  nativo tratan `-<min>` como literal). El resaltado de VS Code reconoce los
  sufijos y los nombres `Int8`…`UInt64` (`sized_ints_contexts.ostrin`).
- **`Float32`** (`Float` = `Float64`): literales `2.5f32`/`1f32` (y `2.5f64`),
  adaptación de literales sin sufijo (`a: Float32 = 0.1`, `x + 0.5`,
  `List<Float32> = [1.5]`, campos y argumentos), aritmética y comparaciones en
  precisión simple (C `float`), sin mezcla implícita con `Float`/enteros
  (`E1041`), conversiones `as Float32 | Float | Int | UIntN…` con comprobación de
  rango. `print`/`to_string` usan la representación más corta que reproduce el
  `f32` (mismo texto en intérprete y nativo: `0.33333334`, `0.1f32 + 0.2f32 = 0.3`).
  `literal_kinds` pasó a valores `LitKind { Int(IntKind), F32 }`.
- **Bug del backend nativo encontrado**: un literal `3.0` se emitía como `3` (el `{}`
  de Rust quita el `.0`), es decir, un literal *entero* en C, y `print(3.0 / 2.0)`
  daba `1` en vez de `1.5`. Ahora se emite con `{:?}` (`3.0`).
- Ejemplos: `float32.ostrin` (comparado nativo/intérprete), `float32_errors.ostrin`.
- Suite: **100 pruebas**, sin warnings. Pendiente del documento 19: `Complex`,
  `Int128`/`UInt128`, `Float16`, y las fases de arrays.

---

## 95. `Array<T>`: arrays N-dimensionales (fase 3 del documento 19) — 2026-09-18

Primer bloque del eje científico, de punta a punta (checker → intérprete → nativo):

- **Tipo** `Array<T>` con `T` ∈ `Int`, `Float`, `Float32` (el intérprete admite además enteros de
  ancho fijo; el nativo solo esos tres). Forma **dinámica** (el rango y las dimensiones se
  conocen en ejecución), row-major, por referencia como `List`. Nunca vacío (toda dimensión ≥ 1).
- **Constructores**: `array(lista)` (listas anidadas hasta profundidad 3 en nativo),
  `zeros(forma)`, `ones(forma)` (`Float`), `full(forma, valor)`, `arange(a, b)` (`Int`,
  `b` exclusivo), `linspace(a, b, n)` (`Float`, el último elemento es exactamente `b`).
  Una función de usuario con el mismo nombre tiene prioridad.
- **Aritmética** `+ - * /` elemento a elemento con **broadcasting** (reglas de NumPy),
  entre arrays o con un escalar del mismo tipo (un literal toma el tipo del elemento:
  `f * 2.0` con `Array<Float32>`); negación. No hay comparaciones ni `@` todavía
  (usar `matmul`/`dot`); las formas incompatibles fallan con `shape mismatch`.
- **Métodos**: `shape rank size/length/count sum mean min max to_list transpose
  reshape sum_axis dot matmul get set`, e indexado `a[i]` en rango 1. `dot` (rango 1,
  devuelve el elemento) y `matmul` (rango 2) se separan para que el tipo de retorno sea
  estático aunque el rango no lo sea. `mean` de `Int` es `Float`.
- **Semántica única**: el intérprete aplica cada operación de elemento con
  `eval_binary_builtin` (overflow de `UInt8`, redondeo de `Float32` idénticos a los escalares) y el
  runtime C (`array_runtime.c`, plantilla instanciada por tipo de elemento) replica el mismo
  orden de acumulación; `print`/`to_string` dan `[[1, 2], [3, 4]]`.
- Mejora colateral: `to_string()` sobre listas, mapas, sets, options, results, records y enums en el nativo.
- Ejemplos: `arrays.ostrin`, `arrays_3d.ostrin` (comparados nativo/intérprete),
  `arrays_errors.ostrin` (7 errores de tipo), `arrays_shape_mismatch.ostrin`
  (test en ambos backends).
- Pendiente: formas estáticas (fase 4), operador `@`, comparaciones que devuelvan `Array<Bool>`,
  vistas/slices sin copia, `Array<Complex>`, SIMD/paralelismo, y álgebra lineal (LU/QR/SVD).
- Suite: **102 pruebas** (5 diferenciales + 97 de integración), sin warnings.

---

## 96. Funciones matemáticas elementales — 2026-09-18

- `sin cos tan asin acos atan sinh cosh tanh exp ln log10 sqrt floor ceil round`,
  `abs`, `pow`, `atan2` y `pi()`, sobre `Float`, `Float32` y **elemento a elemento
  sobre `Array<Float|Float32>`** (`abs` también sobre `Int`, enteros de ancho fijo e
  `Array<Int>`). El tipo de resultado es el del argumento; `Int` nunca se convierte
  implícitamente (`sqrt(2)` es `E1041` con la pista «convert with 'as Float'»).
  `pow`/`atan2` son solo escalares (dos `Float` o dos `Float32`).
- Una función de usuario con el mismo nombre tiene prioridad. Intérprete:
  `interpreter/math.rs`; nativo: `libm` (`sin`/`sinf`, `ostrin_abs_i64`, `Array_T_map` con
  puntero a función). `abs` de un entero en su mínimo falla con overflow en ambos.
- Se puede escribir ya el ejemplo de éxito del prompt maestro:
  `y = sin(x) * exp(-x / 5.0)` con `x = linspace(0.0, 10.0, 6)`.
- **Aviso de reproducibilidad**: `libm` puede diferir en la última cifra entre plataformas
  (Rust vs C, o Windows vs Linux) para `sin`, `exp`, `pow`…; solo `sqrt`, `floor`, `ceil`,
  `round`, `abs` están garantizadas bit a bit. `math_functions.ostrin` redondea
  antes de imprimir por eso. Una decisión pendiente (`docs/design/19`): fijar una
  implementación propia (p. ej. `libm` de referencia) si se quiere reproducibilidad exacta
  entre plataformas.
- Ejemplos `math_functions.ostrin` (nativo = intérprete) y `math_functions_errors.ostrin`.
- Suite: **103 pruebas**, sin warnings.

---

## 97. `Rng`: números aleatorios reproducibles — 2026-09-18

- `g = rng(semilla)` crea un generador (tipo `Rng`, por referencia). xoshiro256** con
  semilla expandida por splitmix64, **implementado por nosotros** en Rust
  (`interpreter/rng.rs`) y en C (`rng_runtime.c`), línea por línea.
- Métodos: `next_float()` (`[0,1)`, 53 bits), `next_int(lo, hi)` (`[lo,hi)`, sin sesgo de módulo),
  `normal()` (normal estándar, método polar de Marsaglia), `rand(forma)`/`randn(forma)`
  (`Array<Float>`), `randint(lo, hi, forma)` y `permutation(n)` (`Array<Int>`, Fisher–Yates).
- **Reproducibilidad bit a bit** entre intérprete y nativo (y entre plataformas): solo se
  usan operaciones enteras y aritmética IEEE correctamente redondeada (`+ - * /`,
  `sqrt`); el logaritmo natural del método polar es propio (`det_ln`: reducción de argumento +
  serie de atanh, ~1e-16), **no** el de `libm`. El nativo se compila con
  `-ffp-contract=off` para impedir multiplicaciones-sumas fusionadas.
  `random.ostrin` (incluye una estimación Monte Carlo de π) da idéntica salida en ambos backends.
- Siguiente paso natural: reutilizar `det_ln` como base de una `libm` propia determinista
  (`exp`, `sin`, `cos`, `pow`) y cerrar el aviso del §96.
- Ejemplos `random.ostrin`, `random_errors.ostrin`. Suite: **105 pruebas**, sin warnings.

---

## 98. Estadística sobre `Array` — 2026-09-18

- Métodos de `Array<Float|Float32>`: `var()`/`std()` (poblacional), `sample_var()`/`sample_std()`
  (ddof = 1), `median()`, `percentile(p)` (`p` en `[0, 100]`, interpolación lineal, `p` siempre `Float`).
  De cualquier `Array`: `cumsum()` y `sort()` (rango 1). De `Array<Int>`: `to_float()`
  (las estadísticas sobre `Int` piden convertir antes: `E1041` con esa pista).
  Funciones `cov(a, b)` y `corr(a, b)` (vectores de rango 1 del mismo tipo flotante).
- **Determinismo**: algoritmos de dos pasadas con orden de acumulación fijo (media, luego
  suma de cuadrados), ordenación estable (merge sort en C; `sort_by` estable en Rust) e
  interpolación con las mismas operaciones; las plantillas `array_stats.c` (nativo) y la
  macro `stats_impl!` (intérprete, una por ancho de flotante) coinciden operación a operación.
  `statistics.ostrin` (incluye 500 normales de `Rng`) da salida idéntica en ambos backends.
- Ejemplos `statistics.ostrin` y `statistics_errors.ostrin` (6 errores). Suite: **107 pruebas**.
- Pendiente de la biblioteca científica: histogramas, regresión (mínimos cuadrados),
  tests de hipótesis/ANOVA, distribuciones (`pdf`/`cdf`), y una `libm` propia determinista.

---

## 99. Librería matemática determinista (`detmath`) — 2026-09-18

Cierra el aviso de reproducibilidad del §96.

- `sin cos tan asin acos atan sinh cosh tanh exp ln log10 pow atan2` ya **no usan la `libm`**:
  se calculan con operaciones enteras, `floor`, `sqrt` y las operaciones IEEE `+ - * /`
  (exactamente redondeadas), en `interpreter/detmath.rs` y su espejo `detmath_runtime.c`
  (mismas fórmulas, mismo orden; el nativo se compila con `-ffp-contract=off`).
  `sqrt floor ceil round abs` siguen siendo las del sistema porque IEEE las define exactas.
  `Float32` calcula en doble precisión y redondea una sola vez.
- Algoritmos: `exp` con reducción de Cody–Waite y serie de Taylor; `ln` por reducción a
  `[√½, √2)` y serie de atanh (compartida con `Rng`); `sin/cos/tan` con reducción por
  cuadrantes (|x| ≤ 1e6; fuera de rango devuelven `NaN`) y núcleos de Taylor; `atan` con
  reducción de argumento; `asin/acos` vía `atan2`; `sinh/cosh/tanh` vía `exp` (serie cerca de 0);
  `pow` con exponenciación binaria exacta para exponentes enteros (`|y| ≤ 1024`) y
  `exp(y·ln x)` en el resto.
- **Precisión medida** contra `math` de Python en 60 entradas aleatorias por función:
  ≤ 1 ulp (`ln`, `log10`), ≤ 2 ulp (`sin cos exp atan asin acos cosh`), 3–5 ulp
  (`tan sinh tanh`), hasta 10 ulp en `pow` no entero. No son correctamente redondeadas; es
  el precio de la reproducibilidad exacta. Mejorable con polinomios minimax sin cambiar el contrato.
- Bug encontrado: el formato de floats grandes en el nativo (`%.0f`) imprimía la expansión
  binaria exacta (`5184705528587073093632`) mientras Rust imprime los dígitos más cortos
  rellenados con ceros (`5184705528587073000000`). Nuevo `ostrin_expand_exp` reproduce el formato de Rust.
- Aviso: `unit` no puede usarse como nombre de variable (palabra reservada del parser).
- Ejemplo `detmath.ostrin` (barridos de todas las funciones; salida idéntica bit a bit
  en intérprete y nativo). Suite: **108 pruebas**, sin warnings.

---

## 100. Regresión, sistemas lineales, histogramas y distribución normal — 2026-09-18

- Funciones sobre `Array<Float>` (`interpreter/regress.rs`, espejo nativo `array_linalg.c`):
  `linfit(x, y)` → `[pendiente, ordenada, r²]`; `polyfit(x, y, grado)` → coeficientes de menor
  a mayor grado (ecuaciones normales); `polyval(coefs, x)` (Horner; `x` escalar o array);
  `solve(A, b)` (eliminación gaussiana con pivote parcial; `singular matrix` si no hay solución
  única); `histogram(datos, bins, lo, hi)` → `Array<Int>` (bins de igual ancho, el extremo superior
  cuenta en el último, lo de fuera se ignora); `erf(x)`; `norm_pdf(x, mu, sigma)` y
  `norm_cdf(x, mu, sigma)` (`x` escalar o array; `sigma > 0`).
- `erf` es determinista (serie de Taylor por debajo de 2, fracción continua de `erfc`
  por encima, saturada en 6) y se suma a `detmath`; `norm_pdf/cdf` la usan.
- Mismo contrato de determinismo que el resto: orden fijo de sumas, sin operaciones fusionadas;
  `calibration.ostrin` (curva de calibración, sistema 3×3, histograma de 2 000 normales de `Rng`,
  `erf` y la normal) da idéntica salida en intérprete y nativo. `solve` recupera `[2, 3, -1]`.
- Ejemplos `calibration.ostrin` y `calibration_errors.ostrin` (6 errores). Suite: **110 pruebas**.
- Límites: solo `Float` (no `Float32`); las ecuaciones normales de `polyfit` pierden precisión con
  grados altos (usar grado ≤ ~8); sin descomposiciones LU/QR/SVD reutilizables todavía.

---

## 101. Sintaxis de arrays: `@`, comparaciones, máscaras y cortes — 2026-09-18

- **`a @ b`** es producto matricial. El parser lo desazucara a `a.matmul(b)` (sin nodo nuevo en
  el AST, así que checker, intérprete y nativo ya lo soportaban); mismo nivel que `*` y `/`,
  asociativo por la izquierda: `a @ b @ c`.
- **Comparaciones** elemento a elemento (`== != < > <= >=`) entre arrays (con broadcasting) o
  con un escalar dan `Array<Bool>`; `and`/`or`/`not` operan elemento a elemento sobre
  `Array<Bool>`. `Array<Bool>` solo admite métodos estructurales y `any()`, `all()`,
  `count_true()` (aritmética o `sum` sobre Bool es `E1041`); `array([true, false])` y
  `full(forma, true)` están permitidos.
- **Máscaras**: `a[a > 0.0]` selecciona los elementos (vector, orden row-major);
  `where(mascara, a, b)` elige elemento a elemento con broadcasting (`a`/`b` pueden ser
  escalares). Una máscara que no selecciona nada es un error en ejecución (los arrays nunca son
  vacíos), igual en ambos backends.
- **Cortes** de vectores: `a[lo until hi]` y `a[lo to hi]` (copia, no vista) y `row(i)`/`col(j)` de
  matrices. Sin vistas ni cortes multidimensionales (`a[:, 0]`) todavía: exigen un modelo de vistas
  prestadas (documento 18) para no copiar.
- Ejemplos `array_syntax.ostrin` (nativo = intérprete), `array_syntax_errors.ostrin` (9 errores),
  `array_empty_mask.ostrin` (test en ambos backends). Suite: **112 pruebas**.

---

## 102. E1101 en el backend nativo y plan de HIR/IR — 2026-09-18

- **«Movido tras enviar» (E1101) ya funciona en nativo** con la misma semántica dinámica
  del intérprete: `send` de un record lo registra por dirección en un conjunto (`moves_runtime.c`,
  tabla hash de direcciones) y leer después una variable que lo contiene falla con
  `'x' was moved into a channel send earlier…`. La generación es en dos pasadas: si el programa
  envía algún record, se regenera con las lecturas instrumentadas (los programas sin ese patrón
  no pagan nada). Se elimina la única entrada de `KNOWN_NATIVE_GAPS` (ahora vacía) y el test
  `moved_after_send_is_a_runtime_error_in_both_backends` comprueba ambos backends. La prueba de
  rechazo del nativo usa `advanced.ostrin` (`Trajectory`).
- Nuevo `docs/design/20-hir-y-ir.md`: qué existe ya (tablas de tipos, sustituciones,
  literales, detector de divergencias), definición del HIR tipado y desazucarado con su
  verificador, IR de bloques básicos y el orden de migración (HIR → backend por familias → IR →
  RC/último uso → cierres → optimizador). Sustituye a la comprobación dinámica de E1101 por un
  análisis estático cuando exista el IR.
- Suite: **113 pruebas**.

---

## 103. HIR: primera etapa construida (árbol tipado + verificador + `--hir`) — 2026-09-18

- `compiler/src/hir.rs`: `lower(items, typed)` construye un **HIR por función** (funciones y métodos
  de `impl`) con un tipo en cada nodo (`HirExpr { ty, kind }`); `verify` comprueba los invariantes
  (nodos con tipo conocido, llamadas a funciones genéricas con sus argumentos de tipo resueltos);
  `dump` lo imprime. CLI: `ostrinc --hir archivo.ostrin` (`--quiet` solo el resumen).
  Las llamadas `recv.metodo(args)` son un nodo propio (`MethodCall`); los operadores, `for`,
  `try` y los patrones se conservan tal cual (el desazucarado es el siguiente paso).
- Base en el checker: `TypedProgram.node_types` (tipo de **cada** nodo, por dirección del nodo en
  el AST, incluidos operandos sin rango propio). **Bug corregido**: los métodos de `impl` y los
  cuerpos por defecto de traits se comprobaban sobre una *copia* del AST, así que sus nodos no
  quedaban registrados; ahora se comprueba el cuerpo original.
- **Cobertura medida** sobre los ejemplos válidos: **4 109 nodos, 53 sin tipo (1,3 %)** y 1
  violación (`wrap(5).is_just()`: una llamada usada como receptor no registra sus argumentos de
  tipo). Antes del arreglo de los métodos eran 1 244 sin tipo. Las que quedan son sobre todo
  lambdas (parámetros sin tipo).
- Test de trinquete `hir_covers_the_examples_with_known_types` (límites 53 / 1).
- Siguiente (documento 20, paso 2): migrar el backend nativo a este HIR por familias de nodos.
- Suite: **114 pruebas**.

---

## 104. HIR sin huecos y comparación del backend por nodo — 2026-09-18

- **HIR**: 4 109 nodos, **29 sin tipo y 0 violaciones** (antes 53 y 1). Los 29 restantes son las muestras
  de sintaxis (`advanced`, `newlines`, nombres sin resolver) y `Ok(7)` sin contexto (`Result<Int, ?>`,
  genuinamente indeterminado). Arreglos: los tipos de las lambdas pasadas a métodos (esa ruta no pasaba
  por `infer_expr`), el operando de `-128i8`, los literales de lista adaptados (`List<UInt8>`), y
  `TypedProgram.call_substs_by_node` (argumentos de tipo por *nodo* de llamada, no solo por rango: el
  receptor `wrap(5).is_just()` ya los tiene).
- **Bug del checker encontrado por la comparación por nodo**: en `fold(inicial, fn(acc, x) { … })` el
  checker daba a la lambda los parámetros en orden `(elemento, acumulador)`; ahora `(acumulador, elemento)`.
- **El backend compara ahora contra el tipo de *cada nodo* del AST** (`node-agreed` en
  `--native-type-report`, misma búsqueda por dirección que usa el HIR), no solo contra los que tienen
  rango: **3 173 nodos coinciden, 0 divergencias** (70 no comparables). Test: `node_agreed > 3000`.
- Trinquetes: HIR ≤ 29 desconocidos / 0 violaciones; suite **114 pruebas**.

---

## 105. HIR: argumentos nombrados y por defecto desazucarados — 2026-09-18

- Al bajar al HIR, toda llamada a una función de usuario, a un método resuelto por el tipo del
  receptor o a un constructor de variante lleva **exactamente sus argumentos posicionales, en orden
  de parámetro**: los nombrados se reordenan y los omitidos se sustituyen por su valor por defecto
  (bajado en el sitio de la llamada, como hace el intérprete). Ejemplo (`--hir`):
  `area(height: 5, width: 4)` → `area(4, 5)`; `label(1)` → `label<T=Int>(1, "#")`.
- `HirProgram.arities` y una comprobación nueva del verificador: una llamada a un callee resuelto
  con nombre superviviente o aridad distinta es una violación. **0 violaciones** en los 4 112
  nodos de los ejemplos (26 sin tipo). Esta lógica es la que hoy duplica `normalize_call_args` en
  el backend; desaparecerá de `codegen.rs` cuando el backend genere desde el HIR.
- Suite: **114 pruebas**.

## 106. Una sola definición de argumentos nombrados/por defecto (HIR ↔ backend nativo)

Primer puente hacia el backend sobre HIR: la lógica que ordena argumentos nombrados y rellena valores por defecto vive ahora en `hir::arrange_arguments` (genérica sobre el tipo del argumento) y la usan tanto el HIR como `codegen::normalize_call_args` (que pasa a ser un adaptador fino). Antes eran dos implementaciones independientes. Pruebas: 6 diferenciales + 98 de integración verdes. El backend todavía genera desde el AST; la migración por familias de nodos (documento 20, etapa 2) continúa con literales/operadores.

## 107. El backend nativo contrasta sus llamadas con la aridad del HIR

`generate_impl` construye el HIR y guarda `hir_arities`; tras normalizar los argumentos de una llamada a función de usuario, si el número no coincide con el del HIR se registra una divergencia (que el test diferencial exige que sea 0). Es la primera comprobación cruzada HIR↔backend en llamadas; sirve de red de seguridad para migrar las llamadas al HIR. 6 + 98 pruebas verdes.

## 108. La comprobación cruzada HIR↔nativo cubre también los métodos

`HirProgram.arities` incluye `Tipo.método` (sin contar `self`); el backend nativo compara con ella tras normalizar los argumentos de cada llamada a método de registro y registra una divergencia si difiere. 6 + 98 pruebas verdes, 0 divergencias.

## 109. Comprobación cruzada HIR↔nativo en constructores de variantes

`gen_variant_args` compara el número de campos de la variante con `HirProgram.arities`; con esto funciones, métodos y constructores quedan contrastados. 6 + 98 pruebas verdes, 0 divergencias.

## 110. Funciones como valores y cierres (intérprete, checker y backend nativo)

Ejemplo: `examples/native_function_values.ostrin` (comparado intérprete↔nativo por la prueba diferencial).

- **Intérprete:** una función con nombre ya se puede usar como valor (`apply(double, 4)`); antes daba `undefined name`.
- **Checker:** una lambda aprende los tipos de sus parámetros del contexto: parámetro de función de usuario con tipo `fn(...) -> ...`, binding anotado, campo de record, cola de una función o de otra lambda (`fn(a) { fn(b) { ... } }`). Bajó de 19 a 1 las expresiones sin tipo y el ejemplo nuevo llega a 0.
- **Nativo:** `CType::Fn` = `OstrinClosure { void* fn; void* env; }` por valor. Cada lambda se eleva a una función C que recibe el entorno; las variables capturadas se **copian** a un entorno en el montón (struct con nombre `OstrinEnv_N`: dos structs anónimos distintos rompían el aliasing estricto con `-O2`). Una función con nombre usada como valor recibe un thunk sin entorno. Se llama por puntero (`f(x)`, `make_adder(3)(4)`), se guardan en listas, records y bindings.
- **Límites conocidos:** la captura es por valor (una variable `mut` modificada después de crear el cierre no se ve dentro; el intérprete comparte el entorno); una lambda sin contexto de tipos (`f = fn(x) { ... }` sin anotar) se rechaza en nativo pidiendo anotación; funciones genéricas no se pueden usar como valor; `?` dentro de una lambda sigue sin soportarse.
- Pruebas: 6 diferenciales (checker↔nativo sin divergencias) y 98 de integración.

## 111. Métodos de String, `List<String>.join` y lectura de CSV

Ejemplo: `examples/string_methods.ostrin` (incluye un lector de CSV escrito en Ostrin; comparado intérprete↔nativo).

- Métodos de `String`: `length` (caracteres), `is_empty`, `trim`, `to_upper`, `to_lower` (ASCII), `contains`, `starts_with`, `ends_with`, `replace` (patrón no vacío), `split` (separador no vacío), `lines` (como Rust: quita `\r` antes de `\n`), `to_int`, `to_float` (`Result<_, String>` con los mismos mensajes que Rust). `List<String>.join(sep)`.
- Tres implementaciones que se reflejan: `interpreter/strings.rs`, `check_string_method` en el checker y `strings_runtime.c` en el nativo (se inserta solo si el programa usa `ostrin_s_*`). `to_float` valida la gramática de Rust antes de `strtod`, así `"5."`, `".5"`, `"1e3"` valen y `"."`/`"abc"` no.
- El lexer acepta `\r` como escape.
- Límite: solo semántica ASCII para mayúsculas/espacios; no hay `Int.to_float()` (se usa `x as Float`).
- Siguiente paso natural: `read_csv`/`DataFrame` sobre esto (columnas tipadas, `describe`, `group_by`).

## 112. `parse_csv`

`parse_csv(texto) -> List<List<String>>` (RFC 4180: campos entre comillas con `""`, comas y saltos de línea dentro del campo, `\n` o `\r\n`, líneas vacías omitidas). Implementado en `interpreter/strings.rs` y espejado en `strings_runtime.c` (`ostrin_s_csv`). Para un archivo: `read_file(ruta)` y luego `parse_csv` sobre el texto (`read_file(p).map(fn(t) { parse_csv(t) })`). Ejemplo comparado entre backends: `examples/csv_parse.ostrin`. Es la base de un futuro `DataFrame`.

## 113. Tabla de columnas (DataFrame mínimo) escrita en Ostrin

`examples/dataframe.ostrin`: `record Table { names, cols }` con `table_from_csv` (sobre `parse_csv`), `col_index`, `floats` (columna → `Array<Float>`), `filter_rows` y `describe` (n, media, desviación, mínimo, máximo), más `corr` entre columnas. Se ejecuta igual en intérprete y nativo, así que sirve además de prueba de que el lenguaje ya alcanza para una biblioteca de datos sin tipos nuevos en el compilador.

Cambio de compilador que salió de aquí: en el backend nativo, `[]` con tipo esperado (`mut xs: List<String> = []`, argumentos, campos) ya se acepta; antes fallaba con «empty list literals aren't supported».

Decisión: la capa de datos crece como biblioteca en Ostrin (módulo `import`), no como tipo interno; el compilador solo se toca cuando la biblioteca choca con un límite del lenguaje.

## 114. Biblioteca de tablas como paquete + arreglos de módulos (tipos y nativo)

`examples/data_project/{tables,app}`: `tables` es un paquete (`pub record Table`, `table_from_csv`, `floats`, `filter_rows`, `describe`, …) y `app` lo importa (`import tables.table`). Se ejecuta igual en intérprete y nativo (test `table_library_module_runs_identically_in_both_backends`).

Errores reales de los módulos que salieron a la luz y se corrigieron:
- `modules.rs` reescribía los nombres en los cuerpos pero **no en tipos de firmas, campos de records, variantes, anotaciones de bindings ni `List<T>[]` vacíos**; un `record` de un módulo no se podía usar como tipo entre módulos (`declared to return 'Table' but its body evaluates to 'pkg.mod::Table'`). Ahora `rewrite_signature`/`rewrite_type` cubren params, retorno, campos, variantes, métodos de impl/trait y anotaciones.
- Nativo: los nombres calificados (`pkg.mod::f`, `pkg.mod::Table`) no son identificadores C. Las funciones se sanean en `c_function_name`; los tipos, con un reemplazo único sobre el C final.
El ejemplo `pkg_project` ya compila también en nativo.

## 115. Agregaciones en la biblioteca de tablas

`tables` gana `unique`, `head`, `group_mean` y `group_count` (agrupar por columna, orden de primera aparición) escritas en Ostrin; el ejemplo `data_project/app` imprime n y media por ciudad, idéntico en intérprete y nativo. Sintaxis a recordar: `and`/`or`/`not` (no `&&`/`!`).

## 116. Gráficos SVG como paquete

`examples/plot_project/{plot,app}`: el paquete `plot` genera SVG (`scatter_svg`, `line_svg`: ejes, extremos rotulados, título) sobre `Array<Float>`, todo en Ostrin, con salida determinista (redondeo a 2 decimales) e idéntica en intérprete y nativo (test `svg_plot_package_runs_identically_in_both_backends`). Guardar a disco: `write_file("grafico.svg", svg)`. Sin dependencias externas ni red. Pendiente: histograma/barras, varias series, leyenda, PNG.

## 117. Diferenciación automática (forward-mode) como paquete

`examples/autodiff_project/{autodiff,app}`: números duales `Dual { v, d }` con `add/sub/mul/div/neg/scale/powi`, `dsin/dcos/dexp/dln/dsqrt`, y sobre las **funciones como valores** (sección 110) `derivative`, `value`, `gradient2` y `newton`. Comprobado a mano: f(x)=x³−2x−5 → f(2)=−1, f'(2)=10, raíz de Newton 2.0945514815423265; gradiente de Rosenbrock en (0.5, 0.5) = (−51, 50). Idéntico en intérprete y nativo (matemática determinista, doc. 19).

Corrección posterior (sección 118): la sobrecarga de operadores **sí existía** (`impl Add/Sub/Mul/Div for T`, ya soportada en intérprete, checker y nativo); el paquete se reescribió con `x * x * x - ...`.

Limitaciones: `Float * Dual` y la negación unaria no están sobrecargadas (se usa `scale`); los tipos importados se usan con `import pkg.mod.{Dual}` (no existe `mod.Dual` en posición de tipo). Reverse-mode y arrays de duales quedan pendientes; 

## 118. El paquete de autodiff usa operadores

Al ir a implementar sobrecarga de operadores descubrí que ya existe (traits integrados `Add`, `Sub`, `Mul`, `Div`, `Eq`, `Ord`; método `add`, `sub`, … resuelto en intérprete, checker y nativo). No hacía falta ningún cambio de compilador: `autodiff.ostrin` ahora declara `impl Add/Sub/Mul/Div for Dual` y las funciones del usuario se escriben como fórmulas (`x * x * x - autodiff.scale(2.0, x) - autodiff.constant(5.0)`). Mismos resultados en ambos backends. Pendiente real: `Float * Dual` (operando izquierdo escalar) y negación unaria sobrecargable.

## 119. Operadores con escalar a la izquierda y negación unaria

Se cierran los dos huecos de la sección 118, en intérprete, checker y nativo:
- `-x` sobre un record/enum llama a su método `neg`.
- `2.0 * x`, `1.0 - x`, `2 + x`… con un tipo de usuario a la derecha llaman al método **reflejado** del tipo (`rmul`, `rsub`, `radd`, `rdiv`), como en Python (`__rmul__`). El escalar llega como segundo argumento.
- Además el checker acepta operandos derechos distintos cuando el tipo declara el método (`v * k` con `fn mul(self, k: Float)`), y devuelve el tipo declarado por el método.
`autodiff.ostrin` los usa: `x * x * x - 2.0 * x - autodiff.constant(5.0)`, `-(1.0 / x)`. Mismos resultados en ambos backends. Límite: los métodos reflejados no pertenecen a un trait (van en un `impl` normal) y no hay `rem`/`pow` sobrecargables.

## 120. Álgebra lineal: `det`, `inv`, `trace`, `eye`

Sobre `Array<Float>`: `det(a)` (eliminación con pivoteo parcial; singular → exactamente 0), `inv(a)` (resuelve `A x = e_j` por columna con el mismo `solve`; singular → error «singular matrix»), `trace(a)`, `eye(n)`. Implementados en `interpreter/regress.rs` y `array_linalg.c` con el mismo orden de operaciones, así que los resultados coinciden **bit a bit**, ruido de redondeo incluido (`examples/linear_algebra.ostrin`, comparado intérprete↔nativo: `A @ inv(A)` da `0.9999999999999997` en ambos). Una función de usuario con el mismo nombre tiene prioridad.

Pendiente: LU/QR/SVD, autovectores, autovalores de matrices no simétricas.

## 121. `matriz @ vector` y `vector @ matriz`

`matmul` (y `@`) acepta ahora `(m, k) @ (k)` → vector de longitud `m` y `(k) @ (k, n)` → vector de longitud `n` (el vector se trata como columna a la derecha y como fila a la izquierda); vector @ vector sigue siendo un error (se usa `dot`). Mismo orden de acumulación en intérprete y nativo, salida idéntica (`examples/linear_algebra.ostrin`).

## 122. `norm` y `eigvals` (matrices simétricas)

`norm(a)`: norma euclídea/Frobenius de un `Array<Float>` de cualquier forma. `eigvals(a)`: autovalores de una matriz **simétrica** por rotaciones de Jacobi cíclicas, ordenados de menor a mayor (error si no es cuadrada o no es simétrica). Solo `+ - * /` y `sqrt` en orden fijo, espejado en `array_linalg.c`: resultados idénticos bit a bit en intérprete y nativo (`[[2,1,0],[1,3,1],[0,1,4]]` → `[1.2679491924311221, 2.9999999999999982, 4.732050807568877]`, exactos 3−… ‑ 3 ‑ 3+√3 salvo 1 ulp). Ejemplo: `examples/linear_algebra.ostrin`.

## 123. Primera familia del backend nativo generada desde el HIR (código escalar)

Nuevo `hir_c.rs`: genera el cuerpo C directamente del HIR para las funciones **elegibles** — todo nodo es `Int/Float/Bool/String` (o `Void` en sentencias) y solo usa locales, literales, aritmética/comparación/lógica, concatenación y `==` de `String`, `if`/`while`/`for` sobre rangos `Int`, `return`/`break`/`continue`, `print` de escalares y llamadas a funciones de usuario con argumentos escalares. Si un nodo no cabe, `generate` devuelve `None` y esa función sigue por el generador del AST (por eso la migración es incremental y segura).

- Conectado en `generate_impl`; `--native-type-report` imprime `hir-generated: N`; `OSTRIN_NO_HIR_CODEGEN=1` fuerza la ruta antigua (para comparar) y `OSTRIN_HIR_DEBUG=1` lista qué funciones van por cada ruta.
- Hoy: 15 funciones de los ejemplos (antes de `print`/`String`: 6). Ratchet en `differential.rs` (≥ 12). Las 6 pruebas diferenciales y las 101 de integración siguen verdes.
- Siguiente familia según el documento 20: records/enums y campos, luego listas/colecciones, patrones, genéricos; cada una ampliando el conjunto elegible y bajando el uso del AST hasta poder borrarlo del generador.

## 124. Records y métodos generados desde el HIR (segunda familia)

`hir_c.rs` ahora entiende **records no genéricos**: literales (mismo esquema que el AST: `malloc` y asignación campo a campo en orden de escritura), lectura y asignación de campos (`->`), records como parámetros/retorno/locales, y **llamadas a métodos** y a funciones de usuario con argumentos de tipo record. Las funciones y los métodos de records (`Tipo.método`) elegibles salen del HIR.

- Para no depender de los tipos internos del backend, el emisor recibe un `World` con nombres de tipos C ya resueltos (firmas de funciones, campos de records, firmas de métodos) y **compara cadenas de tipos C**: si el argumento no coincide con el parámetro (p. ej. un record pasado a un `dyn Trait`), esa función se queda en la ruta del AST.
- Con seguimiento E1101 activo (programas que envían records por canales) los records se desactivan en esta ruta: solo el AST sabe envolver las lecturas.
- Cobertura: 36 funciones/métodos de los ejemplos (15 antes); ratchet ≥ 30. Sigue todo verde (6 diferenciales, 101 de integración).
- Siguientes: enums y `match`, listas/colecciones, closures, genéricos; luego borrar del AST-path lo que ya no use.

## 125. Enums y `match` generados desde el HIR (tercera familia)

`hir_c.rs` genera ahora enums no genéricos (uniones etiquetadas por valor): constructores (`Circle(2.0)`, variantes unitarias como valor), enums como parámetros/retorno/locales y `match` con el mismo esquema que el AST (variable del escrutinio, bandera `matched`, variable de resultado, una `if (!matched && patrón)` por brazo, guardas anidadas, aborto si ningún brazo encaja). Patrones cubiertos: comodín, ligadura, variante unitaria, variante con subpatrones (nombre o posición), literal `Int/Bool/String`, rango `Int`. Records-patrón, `Option/Result` y genéricos siguen por el AST.
Cobertura: 43 funciones y métodos de los ejemplos (36 antes); ratchet ≥ 40; todo verde (6 + 101).

## 126. Punto de retoma (para continuar en otro chat)

**Estado al cierre:** todo commiteado y subido a `main` (`sircalch/Ostrin`). `cargo test` en `compiler/`: 6 diferenciales + 101 de integración en verde. Bitácora al día hasta la sección 125; `ESTADO_Y_PLAN.md` y `CHANGELOG.md` refrescados.

**Lo hecho en esta tanda (secciones 106–125):** funciones como valores y cierres; métodos de `String`, `parse_csv`; paquetes en Ostrin `tables`, `plot` (SVG) y `autodiff`; operadores completos (`neg`, escalar a la izquierda); `det/inv/trace/eye/norm/eigvals` y `matriz @ vector`; arreglos de módulos (tipos en firmas, nombres calificados en nativo); y la **migración del backend nativo al HIR** por familias (`hir_c.rs`: escalares → records/métodos → enums/`match`).

**Siguiente paso recomendado:** cuarta familia en `hir_c.rs`: `Option`/`Result` (`Some/None/Ok/Err`, `try`/`?`, patrones sobre ellos), luego listas/colecciones, cierres, genéricos. Cada familia: ampliar el conjunto elegible, mantener verde `cargo test`, subir el ratchet `hir_generated` en `tests/differential.rs`, y documentar. Al cubrir todo, borrar el camino del AST en `codegen.rs` (habilita RC/último uso, E1101 estático, `--leak-check`: docs 18 y 20).

**Herramientas útiles:** `OSTRIN_HIR_DEBUG=1 ostrinc --emit-c f.ostrin` (qué funciones van por HIR), `OSTRIN_NO_HIR_CODEGEN=1` (fuerza AST), `--native-type-report`, `--typed-report`, `--hir`.

**Trampas conocidas:** editar con scripts Python en el scratchpad (los heredocs de bash rompen comillas/backslashes); los `.rs` del repo usan CRLF (normalizar al editar); sintaxis: `and/or/not`, sin `let`, `match` con comas, sin `
` hasta la sección 111; un ejemplo nuevo no debe pisar uno existente (`native_strings.ostrin` ya existía); los `ostrin.lock` de ejemplos se ignoran por `.gitignore`.

**Pendientes de producto (no empezados):** concurrencia real, gestión de memoria en nativo (hoy `malloc` sin liberar), LU/QR/SVD, autovectores, histograma/barras en `plot`, `Array` de más tipos, WASM/playground, instalador y binarios.

## 127. `Option` y `Result` generados desde el HIR (cuarta familia)

Se cerró la cuarta familia de la migración del backend nativo en `compiler/src/hir_c.rs`.
El emisor HIR reconoce ahora `Ty::Applied("Option", ..)` y `Ty::Applied("Result", ..)` y usa
los mismos nombres de instanciación C que el camino AST (`Option_Int`, `Result_Int_String`, etc.).

- **Construcción**: `Some(x)`, `None` y `None<T>()`, `Ok(x)` y `Err(e)`, incluidos constructores
  con argumentos de tipo explícitos cuando el checker ya resolvió el tipo aplicado.
- **Métodos estructurales**: `is_some`, `is_none`, `unwrap`, `unwrap_or`, `ok_or` en `Option`;
  `is_ok`, `is_err`, `unwrap`, `unwrap_or` y `ok` en `Result`.
- **Patrones**: `Some(x)`/`None` y `Ok(x)`/`Err(e)` en `match`, con ligaduras y subpatrones
  compatibles con los campos internos (`value`, `error`).
- **Propagación**: `try` para `Option` y `Result`, incluyendo el retorno temprano de la variante
  ausente o errónea. `try … catch` y los combinadores que reciben lambdas (`map`, `then`,
  `map_err`) siguen cayendo al AST hasta migrar la familia de cierres.
- **Bindings anotados**: el camino HIR valida que el tipo declarado y el tipo inferido coincidan,
  en vez de rechazar cualquier binding anotado.
- **Tipos locales**: `codegen.rs` recorre los tipos concretos del HIR antes de emitir cuerpos y
  registra sus declaraciones C. Así un `Option` local dentro de `main` no depende de aparecer en
  una firma; `examples/native_hir_option_locals.ostrin` cubre este caso.

La cobertura medida subió de **43 a 61 funciones/métodos generados desde HIR**. El trinquete de
`compiler/tests/differential.rs` queda en **55**. Se añadió `native_hir_handles_option_result_core`
en `compiler/tests/examples.rs`, que exige cobertura HIR en `native_option.ostrin`,
`native_hir_option_locals.ostrin`, `native_result.ostrin`, `native_result_catch.ostrin` y `try_result.ostrin`. La ejecución nativa
de los ejemplos mantiene la misma salida que el intérprete. Validación final de la tanda:
**6 pruebas diferenciales y 102 de integración en verde**.

**Siguiente paso:** quinta familia, listas y colecciones; después cierres y genéricos. Al terminar
esas migraciones se podrá retirar progresivamente el camino AST del backend y comenzar la etapa
de gestión de memoria/último uso prevista en los documentos 18 y 20.

## 128. Listas y colecciones generadas desde el HIR (quinta familia) — 2026-09-18

Se cerró el núcleo de la quinta familia en `compiler/src/hir_c.rs`. El emisor HIR usa las mismas
estructuras y helpers C monomorfizados del backend existente (`List_*`, `Map_*`, `Set_*`), por lo
que la migración no crea una segunda semántica de colecciones.

- **Tipos y construcción**: `List<T>`, `Map<K,V>` y `Set<T>` tienen representación C y nombres
  mangled recursivos (`List_Int`, `Map_String_Int`, `Set_Int`); se generan literales de lista,
  conjunto y mapa, además de `Map<K,V>()` y `Set<T>()` vacíos.
- **Listas**: indexación con comprobación de límites del runtime, `for x in lista`,
  `length`/`count`, `push`, `remove_at` y `List<String>.join`.
- **Mapas**: `get`/`remove` (devuelven `Option<V>` y por tanto se pueden encadenar con
  `unwrap_or` desde HIR), `contains_key`, `count`, `set`, `keys` y `values`.
- **Conjuntos**: `contains`, `add`, `remove` y `count`, incluyendo deduplicación del runtime.
- **Tipos locales**: el registro previo de tipos concretos del HIR cubre también colecciones
  creadas dentro de `main`, sin exigir que aparezcan en una firma. Esto evita que falten las
  declaraciones de structs/helpers cuando una función se genera enteramente desde HIR.

`examples/native_hir_collections.ostrin` y `native_hir_handles_collections_core` cubren la
equivalencia intérprete↔nativo con listas, mutación explícita, indexación, iteración, mapas,
`Option` devuelto por `get`, conjuntos y colecciones vacías. La suite diferencial sube el
trinquete de **55 a 65** funciones generadas desde HIR; la medición actual es **69**. La suite
completa queda en **6 pruebas diferenciales y 103 de integración verdes**.

Los combinadores de colecciones que reciben cierres (`map`, `filter`, `fold`, `any`, `all`,
`find`) siguen usando el fallback AST hasta migrar la familia de cierres. El siguiente paso es
esa migración; después vendrán genéricos y la retirada progresiva del AST del backend.

## 129. Cierres y valores de función generados desde el HIR (sexta familia) — 2026-09-18

Se cerró la sexta familia en `compiler/src/hir_c.rs`. El emisor HIR comparte la representación
`OstrinClosure { fn, env }` del backend nativo y registra sus prototipos/cuerpos auxiliares para
que el programa C final los emita junto con las funciones normales.

- **Cierres**: lambdas con parámetros tipados por el HIR, captura por valor de locales externos,
  entorno C en heap y llamadas indirectas con la misma firma que el backend AST.
- **Valores de función**: una función nombrada puede almacenarse en una variable y llamarse como
  cualquier cierre; se genera un thunk HIR sin entorno.
- **Combinadores**: `List<T>.map`, `filter`, `fold`, `any`, `all` y `find` invocan esos cierres
  desde bucles C generados por HIR, preservando el tipo de salida y el `Option<T>` de `find`.
- **Fallback seguro**: cierres anidados u otras formas todavía no representadas hacen que la
  función completa vuelva al backend AST; no se emite C parcial ni se cambia la semántica.

`examples/native_hir_closures.ostrin` y `native_hir_handles_closures_core` cubren una captura,
un valor de función nombrada y la ejecución idéntica en intérprete y nativo. Con esta familia la
medición global sube a **78 funciones/métodos HIR** y el trinquete diferencial queda en **74**;
la suite completa queda en **6 pruebas diferenciales y 104 de integración verdes**.

El siguiente bloque es la migración de genéricos concretamente instanciados; después se podrá
retirar más código duplicado del camino AST y abordar memoria/último uso.

## 130. Primeras instancias genéricas generadas desde el HIR — 2026-09-18

Se conectó la monomorfización que ya existía en `codegen.rs` con el backend HIR. Una función
genérica sigue validándose una sola vez y conserva una única definición HIR con parámetros
abstractos; cuando el backend descubre una llamada concreta, `hir.rs` crea una copia
especializada con la sustitución que entregó el checker (`CallSubst`) y `hir_c.rs` intenta emitir
esa copia como C.

La especialización cubre recursivamente `Ty::Generic` y los parámetros genéricos que llegan como
`Ty::Named("T")` en las firmas, además de listas, mapas, conjuntos, `Option`, `Result`, funciones
y dimensiones de cantidades. También especializa los tipos de expresiones, bloques, argumentos,
ramas y sustituciones de llamadas anidadas para dejar preparada la siguiente ampliación.

La integración actual demuestra:

- `identity<T>` con `Int`, `String` y un `record` concreto;
- `max<T>` sobre escalares;
- `first<T>(List<T>) -> T` con indexación HIR;
- `maybe<T>(T) -> Option<T>` y `unwrap_value<T>(Option<T>) -> T`;
- registro previo de helpers C para los tipos concretos de una instancia HIR.

`examples/native_hir_generics.ostrin` y `native_hir_handles_concrete_generic_instances` cubren
la equivalencia con el intérprete. Si el cuerpo especializado contiene una llamada genérica
anidada que todavía no tiene una entrada de función HIR/C resoluble, un record o enum genérico
aplicado, o cualquier nodo no migrado, `hir_c::generate` devuelve `None` y la instancia completa
vuelve al backend AST; nunca se mezcla C parcialmente generado.

Resultado de la tanda: **6 pruebas diferenciales y 105 de integración verdes**; la medición global
sube de **78 a 96 funciones/métodos generados desde HIR** y el trinquete diferencial queda en
**90**. El siguiente paso es resolver nombres/prototipos de instancias genéricas dentro de los
cuerpos especializados, y después extender la misma ruta a records/enums aplicados y métodos
genéricos.

## 131. Llamadas genéricas anidadas desde HIR — 2026-09-18

La continuación de la sección 130 cerró la resolución de funciones genéricas dentro de otras
funciones genéricas. `hir.rs` ahora tiene una segunda pasada que recorre el HIR especializado;
para cada `CallSubst` concreto pide al backend el nombre de la instancia C correspondiente,
reescribe el callee y elimina la sustitución abstracta antes de llamar a `hir_c.rs`.

`codegen.rs` reutiliza la misma cola de monomorfización que ya servía al AST. Si la instancia
anidada aún no existe, la crea, registra sus parámetros y retorno, la agrega a la cola y deja
disponible su nombre directo (`identity__Int`) en el `World` HIR. Así se soportan llamadas como:

```ostrin
fn twice<T>(value: T) -> T {
    identity(identity(value))
}
```

También queda cubierta la recursión genérica: la instancia que se está emitiendo se registra
antes de resolver su propio cuerpo. El prefijo normal de funciones fuente (`ostrin_fn_`) no se
aplica a estos nombres ya mangled; los nombres ordinarios conservan exactamente su ruta previa.

`examples/native_hir_generics.ostrin` prueba `twice<T>`, una llamada recursiva genérica y el C
generado contiene la llamada directa a `identity__Int` dos veces. La suite diferencial conserva
equivalencia con el intérprete: **6 pruebas diferenciales y 105 de integración verdes**; la medición
global pasa de **96 a 98**
funciones/métodos HIR y el trinquete queda en **90**.

Queda como siguiente familia la resolución HIR de `record<T>`/`enum<T>` aplicados dentro de estas
instancias y, después, los métodos genéricos. En esos casos el fallback AST sigue siendo obligatorio
hasta que el `World` conozca sus nombres, campos, variantes y métodos concretos.

## 132. Records y enums genéricos aplicados desde HIR — 2026-09-18

Se cerró la siguiente parte de la migración: las instancias concretas de `record<T>` y `enum<T>`
que ya monomorfiza `codegen.rs` ahora también se publican en el `World` de `hir_c.rs`. El puente
conserva el tipo fuente aplicado (`Pair<Int, String>`, `Maybe<Int>`) y lo relaciona con su nombre
C concreto (`Pair__Int_String`, `Maybe__Int`), incluidos argumentos anidados dentro de listas,
mapas, opciones y otras instancias.

El emisor HIR ya puede generar, dentro de una función genérica especializada:

- literales de records genéricos, asignación de sus campos y acceso `obj.field`;
- constructores de variantes genéricas con y sin payload, incluso cuando llevan argumentos de tipo
  explícitos;
- patrones de variantes genéricas en `match`, con sus ligaduras y tags concretos;
- métodos de instancias concretas cuando el registro de métodos ya fue monomorfizado;
- firmas C correctas para records por referencia y enums por valor, sin aplicar el prefijo de
  funciones fuente a nombres ya mangled.

La conversión inversa de tipos nativos a HIR recupera también la forma aplicada de una instancia
anidada, evitando que `Box__Int` se degrade a un nombre opaco cuando aparece como argumento de
otro tipo genérico. Si el seguimiento de movimientos E1101 está activo, los campos de records
siguen forzando el fallback AST para conservar esa comprobación.

`native_generic_types.ostrin` y `native_hir_handles_generic_records_and_enums` cubren records,
enums, campos, `match`, métodos y constructores dentro de instancias especializadas. El C emitido
contiene literales HIR concretos como `Pair__String_Int* __hir_rec`. La suite diferencial conserva
la equivalencia intérprete↔nativo y mide **117 funciones/métodos generados desde HIR**; el trinquete
sube de **90 a 110**.

La siguiente ampliación es completar métodos genéricos complejos y retirar más fallback AST; después
se puede empezar la gestión de memoria nativa y el análisis de último uso previsto en los documentos
18 y 20.

## 133. Métodos genéricos y llamadas de método anidadas desde HIR — 2026-09-18

Se completó el siguiente tramo: los métodos con parámetros propios (`echo<U>`, `swap_in<U>`,
`map<U>`, etc.) ahora reutilizan desde HIR la misma cola de monomorfización que ya usa el backend
AST. El `GenericMethod` conserva el nombre HIR exacto de la declaración y la especialización
concreta de su `impl`, por lo que varias implementaciones con el mismo método (`label` para
`Box<Int>` y `Box<String>`) no pueden cruzar sus cuerpos.

La resolución de llamadas genéricas del HIR ahora cubre dos formas:

- llamadas de función `f<T>(...)`, que se convierten al nombre C concreto;
- llamadas de método `receiver.m<U>(...)`, que registran la firma y el nombre C directo de la
  instancia y quedan listas para el emisor HIR sin prefijo de función fuente.

Esto permite que una función genérica especializada mantenga una llamada de método anidada,
por ejemplo `container.map<U>(value)`, y que un método genérico de un record aplicado devuelva
otro record aplicado (`Box<Int>.swap_in<String> -> Box<String>`) con literal HIR y campos concretos.
Si la forma no está registrada o usa una familia que el emisor todavía no representa, la función
completa conserva el fallback AST.

`native_generic_methods.ostrin` y `native_hir_handles_generic_methods` cubren métodos genéricos
de records normales y aplicados, métodos con múltiples parámetros, argumentos explícitos, métodos
de traits y llamadas anidadas. La suite diferencial mantiene la equivalencia intérprete↔nativo:
**6 pruebas diferenciales y 107 de integración verdes**, con **119 funciones/métodos HIR** y un
trinquete mínimo de **115**.

El siguiente bloque es reducir las formas restantes que dependen del AST y, con la migración HIR
ya dominante, preparar la retirada gradual del generador antiguo antes de entrar en gestión de
memoria/último uso y concurrencia real.

## 134. Primera capa de memoria nativa: registro global y cleanup al salir — 2026-09-18

Se cerró la primera capa de gestión de memoria del backend C. Antes, cada sitio generado
llamaba directamente a `malloc`, `calloc` o `realloc` y el proceso nunca liberaba esos bloques.
Ahora el prelude generado define una API única:

- `ostrin_alloc` para reservas normales;
- `ostrin_calloc` con comprobación de overflow de `count * size`;
- `ostrin_realloc`, que actualiza la entrada registrada cuando cambia la dirección;
- `ostrin_free`, que desregistra antes de liberar;
- `ostrin_mem_cleanup`, que vacía el registro completo.

Cada bloque queda enlazado en un registro interno (`OstrinAllocation`) y `main` instala
`atexit(ostrin_mem_cleanup)`. Se migraron los sitios del generador para records, cierres,
listas, mapas, conjuntos, canales, strings, archivos y buffers de CSV. También se migraron
los runtimes de arrays, álgebra lineal, estadísticas, cantidades, RNG, strings y seguimiento
de movimientos E1101. Los temporales que sí tienen una vida local (`sort`, broadcasting,
`solve`, buffers CSV, etc.) llaman a `ostrin_free`, por lo que no quedan registrados dos veces.

La prueba `native_backend_emits_centralized_memory_cleanup` inspecciona el C generado y exige
la API y el `atexit`; las pruebas nativas existentes ejercitan records, strings, colecciones,
arrays y paquetes. Resultado: **6 pruebas diferenciales y 108 de integración verdes**, y
`cargo check` limpio.

Este bloque no se presenta como ARC completo: todavía no existe conteo de referencias por
alias, análisis de último uso, préstamos, destructores por tipo ni liberación por salida de
ámbito. El registro global es una base segura y comprobable para implementar esas capas sin
mantener asignaciones dispersas. El siguiente bloque de memoria debe añadir metadatos de tipo
y destrucción recursiva; en paralelo se puede continuar retirando el fallback AST.

## 135. Primera bajada HIR → IR con CFG y temporales explícitos — 2026-09-18

Se implementó `compiler/src/ir.rs` y el comando `ostrinc --ir`, como primera parte ejecutable
de la etapa 3 del diseño 20. El backend C todavía no consume esta representación: el objetivo
de esta tanda es crear el lugar correcto para que el análisis de ownership, RC, último uso y
E1101 estático opere sobre valores nombrados y bloques, no sobre expresiones C anidadas.

La IR actual contiene:

- `IrProgram`, `IrFunction` e `IrBlock` con entrada y terminadores;
- temporales numerados (`%0`, `%1`, …), parámetros, constantes, locales y llamadas;
- operaciones unarias/binarias, campos, índices, agregados y valores `phi`;
- CFG explícita para `if`, `while`, `for`, `break`, `continue` y `return`;
- iteradores como `iter_init`, `iter_has_next` e `iter_next`;
- instrucciones `retain`/`release` ya reservadas para la siguiente fase;
- instrucciones `opaque` nombradas para cierres, concurrencia y conversiones que
  todavía necesitan una bajada semántica completa, en lugar de perderse silenciosamente;
  `match` y `try` ya tienen bloques y temporales propios.

El verificador comprueba que no haya bloques sin terminador y que todos los saltos apunten a
bloques existentes. `compiler/tests/examples.rs` cubre `--ir` sobre `native_fibonacci.ostrin`,
incluyendo una rama de bucle y la ausencia de violaciones. La suite queda en **6 pruebas
diferenciales y 109 de integración verdes**; `cargo check` también pasa.

Esto aún no es un backend nuevo ni permite afirmar que Ostrin tenga ARC: es la infraestructura
necesaria para implementarlo. El siguiente paquete debe bajar cierres/concurrencia desde HIR a
CFG real y después añadir el primer pase de ownership (`retain/release` más
`--leak-check`) sin modificar la sintaxis del lenguaje.

## 136. Análisis conservador de ownership y último uso — 2026-09-18

Sobre la IR de la sección 135 se añadió `compiler/src/ownership.rs` y el comando
`ostrinc --ownership-report`. El pase no modifica todavía el programa ni inserta
liberaciones: produce hechos que se pueden inspeccionar antes de activar RC.

Para cada temporal definido, el análisis:

- clasifica como gestionables los records/enums nombrados, colecciones, funciones y valores
  dinámicos;
- recoge todos sus usos en instrucciones y terminadores;
- identifica el último uso cuando todas las referencias permanecen dentro del mismo bloque;
- marca como barreras los valores usados en varios bloques o consumidos por una instrucción
  `opaque`, donde todavía no se conoce el escape real;
- informa valores gestionables sin uso, que serán candidatos a optimización o diagnóstico.

La salida sobre `native_records.ostrin` encuentra 8 valores gestionables y 8 candidatos
lineales. La prueba `compiler_reports_conservative_ownership_facts` protege esta
observabilidad y la suite completa queda en **6 pruebas diferenciales y 110 de integración
verdes**.

La siguiente etapa no debe convertir automáticamente todos los candidatos en `release`:
primero hay que añadir dominadores/joins, propagación por loops, escape de llamadas y tipos de
destructor. Solo después se podrá introducir `retain/release` en una copia de la IR y
comparar `--leak-check` contra el registro global del runtime.

## 137. Match y try convertidos a control de flujo de IR — 2026-09-18

La IR dejó de tratar estas dos familias como una sola instrucción opaca:

- `match` baja el scrutinee, crea una cadena de bloques de prueba por brazo, emite
  `pattern_test`, bindings explícitos para identificadores y campos, saltos de guardas,
  caminos de fallo y un `phi` de convergencia;
- `try` baja el valor protegido a `try_check`, separa los bloques normal y de
  captura, emite `try_value`/`try_error` y converge ambos resultados con `phi`.

La prueba `compiler_lowers_match_and_try_to_explicit_ir_control_flow` cubre
`native_enums.ostrin` y `native_result.ostrin`, y exige que no aparezcan
`opaque match` ni `opaque try`. La suite queda en **6 pruebas diferenciales
y 111 de integración verdes**.

La IR todavía no genera C ni resuelve completamente cierres; la concurrencia tiene ya el
contrato de operaciones, pero el intérprete y el runtime C siguen siendo síncronos. El siguiente
gran bloque es conectar estas operaciones a hilos/canales reales y después hacer la primera
inserción real de `retain/release`.

## 138. Superficie de concurrencia explícita en la IR — 2026-09-18

La bajada HIR→IR ya no representa la concurrencia como `opaque concurrency`:

- `channel` produce `ChannelNew` con capacidad opcional;
- `send`, `receive` y `close` producen operaciones de canal tipadas;
- `join` produce `TaskJoin`;
- `spawn` y `spawn_scope` crean una región de tarea referenciada por
  `Spawn`, con `region_ret` separado del `return` de la función anfitriona.

`native_concurrency.ostrin` y la prueba
`compiler_lowers_concurrency_operations_to_explicit_ir` cubren la superficie completa.
La suite queda en **6 pruebas diferenciales y 112 de integración verdes**.

Esto es una mejora del compilador, no todavía paralelismo nativo: el intérprete puede ejecutar
estas operaciones mediante el scheduler cooperativo documentado en la sección siguiente,
mientras el backend C conserva el modelo síncrono. El próximo paquete de runtime deberá
implementar hilos/canales bloqueantes, cancelación y `select`, manteniendo estas mismas
operaciones como contrato de backend.

## 139. Scheduler cooperativo ejecutable en el intérprete — 2026-09-18

El intérprete dejó de ejecutar `spawn` inmediatamente. Ahora cada tarea se registra como un
estado diferido (`Pending`, `Running`, `Completed` o `Failed`) y conserva su bloque y entorno
capturado. `join()` ejecuta la tarea solicitada y guarda su resultado; un `join` cíclico se
diagnostica como posible deadlock en lugar de recursar indefinidamente.

La espera de canales también tiene semántica ejecutable: `receive()` y `for value in channel`
consumen la cola, reconocen `close()` y ejecutan tareas pendientes cuando todavía no hay datos.
Si no queda ninguna tarea que pueda producir un valor, el intérprete devuelve un error de
bloqueo en vez de colgar el proceso. `spawn_scope` registra su frontera y drena las tareas
creadas dentro del bloque antes de continuar.

`examples/concurrency_scheduler.ostrin` y
`concurrency_scheduler_defers_tasks_and_drains_scopes` protegen el orden observable:
el cuerpo principal avanza antes de la tarea, el scope espera a sus hijos y un productor
pendiente puede alimentar un canal consumido por la tarea anfitriona. La suite queda en
**6 pruebas diferenciales y 113 de integración verdes**.

Esta es concurrencia cooperativa determinista, no paralelismo de CPU. El siguiente paquete
debe reutilizar estos estados y las operaciones de la IR para añadir hilos/canales bloqueantes,
además de cancelación y `select`.

## 140. Scheduler cooperativo alineado en el backend C — 2026-09-18

El backend nativo ya no ejecuta `spawn` como una expresión inmediata. Cada bloque se baja a un
callback C con su entorno capturado por valor y se registra en un scheduler global cooperativo.
Los handles `Task<T>` son heap-owned, tienen estados pendiente/en ejecución/completado y
`join()` ejecuta el callback exactamente una vez; los joins cíclicos producen un error explícito.

Los canales nativos ahora pueden bombear tareas pendientes cuando `receive()` no tiene datos,
y el `for value in channel` usa ese mismo protocolo en lugar de leer la cola directamente.
`spawn_scope` toma una marca del scheduler y drena las tareas creadas dentro de su alcance.
Con esto, `concurrency.ostrin` y `concurrency_scheduler.ostrin` producen la misma salida en
intérprete y C; la prueba dedicada nativa y la comparación diferencial protegen esa paridad.

La paridad actual es cooperativa y determinista: no crea hilos del sistema operativo, no hace
paralelismo de CPU y todavía no implementa cancelación ni `select`. La suite queda en **6
pruebas diferenciales y 115 de integración verdes**. El siguiente bloque de runtime debe
añadir el modo multi-hilo de forma opt-in o por backend, con canales sincronizados y una
política clara para E/S y cancelación.

## 141. Primera bajada conservadora de ownership sobre la IR — 2026-09-18

La IR ahora tiene dos herramientas de ownership que no dependen del texto C:

- `--ownership-check` recorre los usos de valores gestionados y emite `OSTRIN-E1101` cuando
  un valor de tipo agregado/referencia se usa después de `ChannelSend`. La salida conserva la
  función, el temporal y las posiciones de bloque/instrucción para que el diagnóstico sea
  inspeccionable antes de conectarlo a spans de origen.
- `--ownership-ir` clona la IR e inserta `retain` cuando un agregado conserva un campo
  gestionado o cuando `Field`/`Index`/`PatternBind`/`Phi` producen un alias gestionado; inserta
  `release` solo después de un último uso lineal en una transferencia que ya tiene contrato:
  almacenamiento local sin lecturas posteriores o envío por canal. Los valores que cruzan
  bloques, pasan por `opaque`, llamadas o terminadores quedan contados como
  `unresolved-values`.

`ownership_linear.ostrin` protege la inserción de un marcador de liberación y
`moved_after_send.ostrin` protege el diagnóstico estático. La pasada no cambia todavía el
backend C ni pretende ser ARC completa: faltan `retain` en copias/aliases, dominadores y
loops, análisis de escape, destructores por tipo y ciclos. La suite queda en **6 pruebas
diferenciales y 116 de integración verdes**.

El siguiente bloque de memoria debe definir esos contratos de retain para `Aggregate`,
`Call`, `Field`, `Phi` y retornos, y después hacer que el backend C consuma la IR transformada.

## 142. E1101 estático integrado y records inmutables compartibles — 2026-09-19

El análisis de movimiento pasó de ser una herramienta explícita a formar parte del contrato
normal del compilador:

- `ostrinc --run`, `--check`, `--emit-c` y `--compile` bajan el programa a HIR/IR y rechazan
  antes de ejecutar o generar C cualquier uso de un valor movible después de `ChannelSend`.
- El diagnóstico estable es `OSTRIN-E1101` e incluye temporal, tipo y bloques/instrucciones
  de envío y uso. `--ownership-check` sigue disponible para inspeccionar únicamente el informe.
- La clasificación ya distingue records y enums con estado mutable directo o anidado de los
  records inmutables. `List`, `Map` y `Set` siguen siendo valores gestionados por identidad,
  incluso cuando sus elementos son escalares.
- Intérprete y backend nativo aplican la misma distinción: un `record` inmutable se puede
  compartir por canal, mientras que uno mutable conserva la regla de transferencia única.
- Se añadió `examples/immutable_record_channel.ostrin` y una prueba de paridad intérprete/C;
  la prueba de `moved_after_send.ostrin` ahora verifica que ambos puntos de entrada fallen
  estáticamente, sin compilar un ejecutable inválido.

La suite queda en **6 pruebas diferenciales y 117 de integración verdes**. El siguiente bloque
de memoria sigue siendo completar contratos de `retain`/`release` para llamadas, retornos,
joins y loops, y hacer que el backend C consuma la IR transformada; E1101 ya está conectado
al flujo normal, pero la ARC completa todavía no existe.

## 143. ABI de ownership nativo y `--leak-check` — 2026-09-19

El runtime C ya tiene la primera superficie ejecutable para el próximo lowering de memoria:

- Cada bloque registrado mantiene un contador `refs` inicializado en uno; `ostrin_retain` lo
  incrementa de forma saturante y `ostrin_release` lo decrementa, liberando por la misma ruta
  registrada cuando llega a cero.
- `--leak-check` se puede combinar con `--emit-c` o `--compile`. El programa nativo imprime
  `live_allocations`, `peak_allocations` y `total_allocations` antes de que `atexit` ejecute la
  limpieza global.
- La API todavía no se inserta automáticamente en cada copia de record, colección, llamada o
  retorno: el backend C actual sigue usando el registro global como red de seguridad. Esto es
  intencional hasta que la IR transformada sea la fuente de emisión y pueda respetar joins,
  loops, escapes y destructores sin liberar dos veces.
- Se añadió una prueba que inspecciona el C generado y verifica la presencia del ABI y del
  informe. La suite queda en **6 pruebas diferenciales y 118 de integración verdes**.

Siguiente paso: conectar `--ownership-ir` con una emisión mínima de retain/release para
temporales de records y colecciones en bloques lineales, dejando llamadas, phi, loops y escapes
marcados como barreras hasta que sus contratos estén implementados.

## 144. Argumentos de programa como capacidad de biblioteca estándar — 2026-09-19

Ostrin ahora expone `args()` como una primitiva de aplicación en ambos backends:

- El checker reconoce `args()` sin argumentos y devuelve `List<String>`.
- El intérprete recoge los argumentos situados después del separador `--` de la invocación
  de `ostrinc`, evitando mezclar opciones del compilador con los datos del programa.
- El backend nativo genera `main(int argc, char** argv)`, conserva `argc/argv` en el runtime y
  construye la misma `List<String>` a partir de `argv[1..]`.
- `examples/args.ostrin` y la prueba `program_arguments_match_between_interpreter_and_native`
  validan valores reales (`uno`, `dos`) y la paridad de salida.

La suite queda en **6 pruebas diferenciales y 119 de integración verdes**. Esta base permite
crear CLI Ostrin reales; el siguiente bloque de stdlib puede añadir entorno, rutas y formato
estructurado sin cambiar el contrato de ejecución.

## 145. Entorno y rutas como biblioteca estándar multiplataforma — 2026-09-19

La biblioteca estándar incorpora dos primitivas pequeñas pero necesarias para que los programas
Ostrin puedan dejar de depender de valores fijados en el código fuente:

- `env(name)` es reconocida por el checker como `Option<String>`, consulta el entorno del proceso
  en el intérprete y usa `getenv` en el runtime C. Un nombre ausente devuelve `None` en ambos casos.
- `path_join(left, right)` normaliza separadores iniciales/finales y devuelve una ruta compuesta
  con `/`, con el mismo comportamiento en Windows, Linux y macOS porque la operación pertenece al
  lenguaje y no a una concatenación específica del host.

El ejemplo `examples/env_path.ostrin` y la prueba diferencial
`environment_and_paths_match_between_interpreter_and_native` comprueban el valor real de entorno,
la representación de `Option<String>` y la salida de la ruta en ambos backends. La suite queda en
**6 pruebas diferenciales y 120 de integración verdes**.

Esto completa el primer bloque de entorno/rutas de la stdlib. Aún faltan un formateador estable,
fechas, JSON, red y una colección hash eficiente; no se presenta este bloque como un sistema de
paquetes completo.

## 146. Igualdad estructural de colecciones y tipos suma — 2026-09-19

La comparación `==`/`!=` deja de ser una limitación del backend nativo para los tipos de datos
compuestos del núcleo:

- `List<T>` compara longitud y elementos en orden, recursivamente.
- `Map<K,V>` compara pares clave/valor sin depender del orden de inserción.
- `Set<T>` compara pertenencia, también sin depender del orden interno.
- `Option<T>` y `Result<T,E>` comparan primero la variante (`Some`/`None`, `Ok`/`Err`) y después
  su payload cuando existe.

El intérprete usa la misma operación recursiva que ya emplean `contains`, `find`, `Map` y `Set`;
el backend C genera helpers tipados `ostrin_eq_*`, registra las instancias anidadas y conserva
la semántica para valores dentro de records, enums y colecciones. `Array<T>` también tiene helper
estructural para aserciones, mientras que sus operadores ordinarios mantienen su semántica
científica elemento a elemento y devuelven una máscara.

`examples/structural_equality.ostrin` y la prueba
`structural_equality_matches_between_interpreter_and_native` cubren las siete salidas esperadas
en ambos backends. La suite queda en **6 pruebas diferenciales y 121 de integración verdes**.

## 147. Formateo y filesystem mínimo para programas CLI — 2026-09-19

La stdlib gana un bloque pequeño de utilidades de aplicación, disponible con la misma firma y
semántica en el intérprete y en el backend C:

- `format(template, values)` recibe un `String` y un `List<String>`. Cada marcador `{}` consume
  el siguiente valor; si falta un valor, el programa termina con un error de runtime explícito.
- `cwd()` devuelve el directorio de trabajo actual como `String`; el backend nativo usa una consulta
  que crece dinámicamente y distingue `_getcwd` en Windows de `getcwd` en plataformas POSIX.
- `file_exists(path)` devuelve `Bool` y comprueba que la ruta sea un archivo legible mediante la
  misma convención de proceso en ambos backends.

`examples/format_filesystem.ostrin` y la prueba
`formatting_and_filesystem_builtins_match_between_interpreter_and_native` validan una expansión
con tres placeholders y consultas reales de directorio/archivo. La suite queda en **6 pruebas
diferenciales y 122 de integración verdes**.

El formateador es deliberadamente acotado: todavía no ofrece especificadores numéricos, escape de
llaves, JSON ni interpolación tipada. Es una base estable para añadir esas capas sin inventar una
API distinta por backend.

## 148. Índice hash para `Map` con orden estable — 2026-09-19

`Map<K,V>` deja de hacer una búsqueda lineal para las claves escalares que el backend puede
identificar de forma segura (`Int`, enteros de ancho fijo, `Bool`, `String`, `Float` y `Float32`):

- El intérprete conserva sus entradas en orden de inserción y mantiene un índice hash secundario;
  actualizar o borrar una entrada reconstruye el índice para que los índices de posiciones nunca
  queden obsoletos.
- El backend C genera buckets de direccionamiento abierto, rehash con umbral de carga del 70 %,
  crecimiento geométrico y reconstrucción después de `remove` tanto para `Map` como para `Set`.
  `keys()`/`values()` y la iteración de conjuntos siguen devolviendo el orden de inserción anterior.
- Las claves y elementos compuestos todavía usan el camino lineal correcto. Esto es intencional: el trait
  `Hash` existe en el vocabulario del checker, pero todavía no se exige ni se genera automáticamente
  para records y colecciones; se completará antes de declarar hashables los tipos de usuario.

`examples/hash_map_stress.ostrin` inserta 51 claves, actualiza una, elimina otra y verifica
consultas posteriores. `hash_map_scalars_match_between_interpreter_and_native` comprueba la misma
salida en ambos backends. La suite queda en **6 pruebas diferenciales y 123 de integración verdes**.

## 149. Destrucción tipada y ownership explícito en el backend nativo — 2026-09-19

El runtime C da ahora un paso ejecutable entre el registro global y la ARC automática:

- Cada allocation puede registrar un callback `drop(void*)` junto con su contador de referencias.
  `ostrin_release` retira el bloque de la tabla, ejecuta su destructor y libera el almacenamiento
  cuando la cuenta llega a cero; `ostrin_realloc` conserva el callback y la identidad del bloque.
- Records, `List<T>`, `Map<K,V>`, `Set<T>` y `Channel<T>` registran destructores tipados. Esos
  destructores liberan buffers internos y sueltan los elementos/campos que son referencias.
  Las inserciones en listas, mapas, sets y canales retienen las referencias almacenadas; los
  records retienen sus campos y liberan el campo anterior al reasignarlo.
- `clone(value)` incrementa la referencia nativa para tipos gestionados y devuelve el mismo
  valor con identidad compartida. `drop(value)` libera explícitamente una referencia; los tipos
  escalares siguen siendo valores sin coste. El intérprete expone la misma superficie semántica
  de alias, mientras que su gestión de memoria la resuelve Rust/Rc.
- `examples/ownership_primitives.ostrin` crea una lista, clona el alias y libera ambos dueños.
  La prueba nativa verifica la salida `3` y `live_allocations=0` con `--leak-check`.

La batería queda en **6 pruebas diferenciales y 124 de integración verdes**. Este hito no se
debe confundir con ARC completa: las copias ordinarias, retornos, `phi`, escapes, cierres y
salidas de ámbito todavía no reciben automáticamente `retain`/`release` desde la IR. El próximo
bloque recomendado es conectar los contratos lineales de `ownership.rs` a una emisión controlada
para temporales locales simples, manteniendo llamadas, bucles, joins y escapes como barreras.

## 150. Ownership automático lineal en HIR/AST — 2026-09-19

El backend nativo ya consume una primera parte del contrato de ownership sin depender de
`clone`/`drop` escritos por el usuario:

- Los locales directos de cada callable que reciben una referencia nueva quedan registrados como
  dueños; una asignación desde un identificador, campo o índice emite `ostrin_retain` antes de
  almacenar el alias.
- Una reasignación evalúa el valor nuevo en un temporal, retiene el préstamo si corresponde,
  libera el valor anterior y solo después actualiza el binding. Esto evita perder el valor nuevo
  cuando la expresión es `x = x`.
- Los retornos de referencias pasan por un temporal: un local propio transfiere su referencia al
  llamador y un parámetro/campo/índice prestado recibe un retain antes de limpiar los dueños
  restantes. Los retornos tempranos y el retorno final usan la misma ruta.
- La política está implementada tanto en `hir_c.rs` como en el fallback AST de `codegen.rs`.
  Los records construidos desde HIR usan también allocation registrada y destructores tipados,
  reteniendo sus campos de referencia; esto evita devolver records con listas ya liberadas.
- Se añadió `examples/ownership_auto.ostrin`, que prueba alias, reasignación y retorno de un
  parámetro, y la prueba nativa exige salida `3/1/1` y `live_allocations=0`.
- Las compilaciones nativas de tests usan nombres únicos para sus fuentes C temporales; antes
  podían sobrescribirse al correr pruebas en paralelo.

La batería queda en **6 pruebas diferenciales y 125 de integración verdes**. El alcance sigue
siendo deliberadamente lineal: bindings creados dentro de scopes anidados, `phi`, loops, cierres,
contenedores suma y escapes complejos esperan la bajada completa desde `ownership.rs` hacia el
backend C.

## 151. Hilos nativos y canales bloqueantes opt-in — 2026-09-19

El backend nativo incorpora una primera ejecución concurrente real sin cambiar la semántica
determinista que usa la suite diferencial:

- `--native-threads` solo se habilita junto con `--emit-c` o `--compile`; sin el flag,
  `spawn` conserva el scheduler cooperativo existente.
- El runtime C abstrae `pthread` en POSIX y `CreateThread` en Windows, con mutexes,
  variables de condición y join seguro para `Task<T>`.
- `Channel<T>` tiene un camino bloqueante con buffer protegido: `send` despierta receptores,
  `receive` espera mientras el canal está vacío y `close` despierta a quienes esperan para
  que puedan observar `None`.
- La tabla global de allocations usa un mutex para que retain/release/realloc y los
  destructores tipados no corran concurrentemente sobre la misma lista.
- Los entornos capturados por una tarea retienen sus valores gestionados al crearse y liberan
  esas referencias al terminar el callback; el destructor de la tarea espera el hilo antes de
  destruir sus primitivas de sincronización.
- `examples/native_threads.ostrin` y su prueba nativa ejercitan un receive que bloquea hasta
  un productor real, `close`, `join` y `--leak-check`; esperan `7` y
  `live_allocations=0`.

La batería queda en **6 pruebas diferenciales y 126 de integración verdes**. Quedan para el
siguiente bloque de concurrencia `select`, cancelación y un administrador de grupos nativos
para que `spawn_scope` tenga garantías completas también en el modo con hilos.

## 152. Entrada de proyecto y lockfiles portables — 2026-09-19

El flujo de paquetes local deja de depender de pasar siempre el archivo de entrada a mano:

- `ostrinc --project DIR` lee `DIR/ostrin.toml` y usa el campo `entry`; también acepta la
  ruta directa al manifiesto.
- `write_lockfile` ordena las dependencias por nombre para que la salida sea determinista.
- Las dependencias locales se escriben como rutas relativas al directorio del manifiesto cuando
  la relación es representable; se evita incrustar la ruta absoluta del checkout.
- Las dependencias `git` siguen siendo reconocidas pero no se descargan automáticamente, de
  modo que una compilación normal no introduce efectos de red.
- La prueba de proyecto ejecuta `examples/pkg_project/main_app` solo con `--project` y
  verifica la ruta `../shared_lib` en el lockfile.

La batería queda en **6 pruebas diferenciales y 127 de integración verdes**.

## 153. Distribución reproducible del compilador como WASI — 2026-09-19

Se verificó que `ostrinc` compila en release para `wasm32-wasip1` con el toolchain actual.
El nuevo workflow `.github/workflows/wasi.yml`:

- instala el target WASI en Ubuntu;
- construye `compiler/Cargo.toml` con `cargo build --target wasm32-wasip1 --release`;
- empaqueta `ostrinc.wasm`, un README de runtime y su SHA-256 en un tarball;
- publica el tarball como artifact en ejecuciones manuales y en tags `v*`.

La frontera se documenta explícitamente: es distribución del compilador para hosts WASI, no
todavía compilación de programas Ostrin a WASM ni un playground de navegador. El backend de
programas sigue generando C, y el próximo paso WASM es separar un runtime sin pthreads ni APIs
de proceso para poder probar un programa pequeño sobre WASI.

## 154. Hash estable como builtin de stdlib — 2026-09-19

La biblioteca estándar expone `hash(value) -> Int` para las claves escalares que ya soportan
el índice hash de `Map` y `Set`:

- El checker acepta `Int`, enteros fijos, `Bool`, `Float`, `Float32` y `String`, y
  rechaza colecciones, records y otros valores compuestos hasta que exista un contrato
  `Hash` formal para tipos de usuario.
- El intérprete sustituyó `DefaultHasher` por splitmix64 para números y FNV-1a para cadenas,
  eliminando una dependencia del hasher interno de Rust.
- El backend C usa los mismos algoritmos y trata `Float32` por sus bits de 32 bits, no por
  una conversión accidental a `double`.
- `examples/hash_builtin.ostrin` y la prueba diferencial comparan seis valores entre ambos
  backends; la salida incluye hashes negativos para confirmar que la conversión `u64 -> Int`
  conserva los bits.

La batería queda en **6 pruebas diferenciales y 128 de integración verdes**.

## 155. Registro nativo de tareas sincronizado y drenado seguro — 2026-09-19

El runtime C del backend nativo separa ahora la sincronización del heap y la del
registro global de tareas:

- `ostrin_tasks_mutex` protege la lista y el ordinal del scheduler cuando hay hilos
  reales;
- cada tarea registrada conserva una referencia propia, y el polling conserva una
  referencia temporal mientras invoca el callback;
- `join` desregistra la tarea terminada, mientras que `spawn_scope` y la limpieza de
  salida retiran y liberan los nodos restantes sin dejar punteros obsoletos;
- la prueba de hilos nativos y la suite diferencial siguen verdes: **128 pruebas de
  integración y 6 diferenciales**.

La verificación manual de `examples/concurrency_scheduler.ostrin` con
`--native-threads --leak-check` mantiene la salida observable correcta (`main`, `task`,
`42`, `scope-body`, `scope-task`, `7`). Todavía reporta una asignación viva: es el
binding local `child` del scope anidado, porque la emisión de ownership cubre locales
directos de funciones pero aún no todos los bindings creados dentro de regiones
anidadas. Se conserva como brecha explícita de la siguiente etapa de lowering de
ownership, no como un problema del registro concurrente.

## 156. Ownership de bloques anidados — 2026-09-19

La bajada nativa añade frames de ownership para cada expresión de bloque anidado
generada dentro de una función:

- los bindings locales de referencia se registran en el frame del bloque, no en el
  frame de la función;
- antes de devolver el valor del bloque, el emisor lo guarda en un temporal, retiene
  los resultados prestados y libera los locales que no escapan;
- el mecanismo cubre el handle `child` creado dentro de `spawn_scope` y conserva el
  orden observable del scheduler nativo;
- además, `spawn` registra la tarea antes de arrancar el hilo del SO, eliminando la
  ventana en la que un hilo podía terminar antes de entrar al registro.

La prueba `native_threads_scope_drain_releases_nested_task_handles` verifica
`concurrency_scheduler.ostrin` con `--native-threads --leak-check`: salida idéntica y
`live_allocations=0`. La batería queda en **6 pruebas diferenciales y 129 de
integración verdes**. La ARC completa sobre la IR, escapes complejos y cancelación
siguen pendientes.

## 157. Smoke test del compilador WASI — 2026-09-19

La distribución WASI ya no solo compila el módulo: el workflow wasi.yml lo arranca
bajo la API node:wasi de Node con --check examples/hello.ostrin y un preopen del
workspace. Esto verifica simultáneamente:

- que ostrinc.wasm exporta un entrypoint ejecutable en WASI preview1;
- que recibe argumentos de CLI;
- que puede leer un archivo Ostrin mediante el filesystem preabierto;
- que devuelve código de salida cero después del type-check.

La misma prueba se ejecutó localmente sobre el artefacto wasm32-wasip1 y produjo
OK — no se encontraron errores de tipo (2 elemento(s)). La etapa sigue siendo
distribución del compilador WASI, no compilación de programas Ostrin a WASM.

## 158. Hash estructural para Option y Result — 2026-09-19

El builtin hash amplía su contrato más allá de escalares:

- Option<T> combina una etiqueta estable (Some/None) con el hash del payload;
- Result<T, E> combina la etiqueta (Ok/Err) con el hash del valor activo;
- el checker acepta estas formas solo cuando sus payloads son recursivamente
  hashables; listas, mapas, sets y otros valores de identidad siguen rechazados
  hasta definir el trait Hash de usuario;
- el intérprete y el backend C comparten la misma mezcla splitmix64 y los mismos tags.

examples/hash_builtin.ostrin cubre cuatro valores estructurales adicionales y la
prueba existente de paridad intérprete↔nativo los ejecuta en ambos backends. El
siguiente paso de stdlib es formalizar Hash para colecciones. La batería
queda en 6 pruebas diferenciales y 130 de integración verdes.

## 159. derive(Hash) para records — 2026-09-19

El contrato Hash de usuario queda abierto para records:

- un record puede declarar derive(Hash);
- el checker comprueba recursivamente sus campos y solo acepta escalares,
  Option/Result hashables u otros records con derive(Hash);
- el intérprete combina Record::<nombre> con los hashes de los campos en orden de
  declaración;
- el backend C emite la misma combinación estable y rechaza records no hashables.

hash_builtin.ostrin ahora cubre HashPoint y un HashEnvelope con un campo Option<Int>.
La prueba negativa confirma que un record sin derive(Hash) no puede pasarse al
builtin. Enums, listas, mapas y sets siguen fuera del contrato de Hash de usuario.

## 160. derive(Hash) para enums — 2026-09-19

El contrato Hash de usuario cubre también enums no genéricos:

- un enum puede declarar derive(Hash);
- el checker exige que todos los campos de todas sus variantes sean
  recursivamente hashables, incluyendo records/enums derivados;
- el intérprete combina `Enum::<tipo>::<variante>` con los campos en orden de
  declaración;
- el backend C genera la misma cadena de variante, etiqueta y combinación estable.

hash_builtin.ostrin cubre una variante vacía y otra posicional. La prueba negativa
confirma que un enum sin derive(Hash) se rechaza en compilación. Los enums genéricos,
listas, mapas y sets siguen fuera de este contrato de usuario.

## 161. Hash estructural para colecciones — 2026-09-19

El builtin hash acepta ahora colecciones compuestas cuando sus elementos son
recursivamente hashables:

- `List<T>` combina la etiqueta `List` y los elementos en orden;
- `Map<K, V>` combina cada par clave/valor con acumulación independiente del
  orden de inserción y del tamaño;
- `Set<T>` aplica la misma acumulación independiente del orden a sus elementos;
- intérprete y backend C comparten las etiquetas, la mezcla y el tratamiento de
  los valores anidados.

hash_builtin.ostrin verifica también que dos mapas y dos sets con inserciones
invertidas tengan el mismo hash. Funciones, arrays, canales, tareas y colecciones
con payload no hashable siguen rechazadas por el checker.

## 162. Ownership por iteración y rama en AST/HIR — 2026-09-19

El cleanup nativo de valores gestionados deja de limitarse al scope lineal y a bloques
de expresión:

- el emisor AST abre frames de ownership para cada `while`/`for` y para cada rama;
- el emisor HIR aplica el mismo contrato a sus bloques anidados;
- los bloques de expresión conservan la transferencia de una cola gestionada o hacen
  `retain` cuando esa cola es un préstamo externo antes de liberar sus locales;
- `break` y `continue` liberan los frames que abandonan antes de saltar;
- los elementos gestionados de iteraciones sobre listas/canales reciben el retain/release
  correspondiente al préstamo o transferencia que representa la iteración.

Se añadieron `ownership_loops.ostrin` para la ruta HIR y `ownership_loops_ast.ostrin`
para forzar el fallback AST. Ambos verifican salida estable y `live_allocations=0`; la
batería queda en 6 pruebas diferenciales y 132 de integración verdes. Escapes complejos,
dominadores y ownership completo sobre la IR siguen pendientes.

## 163. Índices de Map/Set para claves compuestas — 2026-09-19

El hash estructural deja de ser solo una operación del builtin `hash` y pasa a alimentar
los índices internos de `Map` y `Set` en ambos backends:

- el intérprete reindexa cada colección después de construirla o mutarla y calcula de forma
  recursiva los hashes de `List`, `Map`, `Set`, `Option` y `Result`;
- el backend nativo conserva su tabla de buckets para colecciones anidadas, con las mismas
  etiquetas y mezcla que el intérprete; si una clave puede mutar por alias, reconstruye los
  buckets antes de buscar para no dejar hashes obsoletos;
- records y enums definidos por el usuario solo usan buckets cuando tienen `derive(Hash)` y
  `derive(Eq)` compatibles y no declaran un `equals` personalizado; en los demás casos la
  búsqueda cae a un recorrido lineal correcto, evitando falsos negativos;
- `examples/hash_map_composite.ostrin` verifica `Map<List<Int>, String>`, `Set<List<Int>>`,
  actualización/deduplicación y una igualdad personalizada más amplia que el hash, en modo
  intérprete y nativo.

La batería queda en **6 pruebas diferenciales y 133 de integración verdes**. Sigue pendiente
extender la distribución WASM al backend de programas Ostrin.

## 164. Restricciones Hash + Eq en Map/Set — 2026-09-19

El contrato de las colecciones deja de ser solo una convención de los backends:

- el checker exige `Hash + Eq` para cada clave de `Map` y cada elemento de `Set`;
- la comprobación recorre recursivamente `List`, `Map`, `Set`, `Option`, `Result` y los campos
  de records/enums derivados, respetando los bounds de parámetros genéricos;
- `derive(Hash)` más `derive(Eq)` o un `impl Eq` explícito habilitan tipos definidos por el usuario;
  una igualdad personalizada sigue usando el fallback lineal seguro del runtime;
- `native_collections.ostrin` declara ahora `Hash` para su `Point`, y una prueba negativa cubre
  por separado la falta de `Hash` y de `Eq`.

La batería queda en **6 pruebas diferenciales y 134 de integración verdes**. La próxima brecha
grande continúa siendo el backend de programas Ostrin para WASM, después de la distribución WASI
ya disponible para el compilador.

## 165. Select determinista y paridad de canales — 2026-09-19

La superficie de concurrencia incorpora selección entre varios canales sin añadir una
gramática nueva:

- `select([channel1, channel2, ...]) -> Option<T>` exige en el checker una lista homogénea
  de `Channel<T>` y devuelve el primer valor disponible en el orden de la lista;
- un canal cerrado y vacío cuenta como listo y devuelve `None`, mientras que los valores
  pendientes se consumen antes de observar el cierre;
- el intérprete mantiene el scheduler cooperativo determinista: si ningún canal está listo,
  ejecuta una tarea pendiente y vuelve a inspeccionar la lista;
- el backend C genera `Channel<T>_try_receive`, con mutex en `--native-threads`, y el
  builtin cede el hilo entre intentos; el modo nativo cooperativo usa `ostrin_poll_all()`;
- `examples/concurrency_select.ostrin` verifica un productor que despierta la selección,
  prioridad de lista y equivalencia intérprete↔nativo, incluyendo `--native-threads`. La
  prueba negativa confirma que una lista de valores no se acepta como canales.

La batería queda en **6 pruebas diferenciales y 136 de integración verdes**. La próxima
brecha de concurrencia es cancelación explícita y grupos nativos completos; la distribución
WASM del backend de programas sigue pendiente después del compilador WASI.

## 166. Cancelación segura de tareas pendientes — 2026-09-19

La API de tareas deja de tener la cancelación solo como pregunta de diseño, con una
semántica deliberadamente acotada:

- `task.cancel() -> Bool` solo puede cambiar una tarea en estado `Pending` a `Cancelled`;
  devuelve `true` si la transición ocurrió y `false` si la tarea ya empezó o terminó;
- `join()` sobre una tarea cancelada produce `task was cancelled`, sin inventar un valor
  de retorno ni ocultar la razón de que el trabajo no se ejecutó;
- el intérprete y el scheduler nativo cooperativo comparten la transición determinista;
  `--native-threads` protege la transición con el mutex del `Task<T>`, pero no intenta
  detener un hilo que ya está ejecutando código arbitrario;
- `examples/concurrency_cancel.ostrin` cubre cancelación idempotente (`true`, luego
  `false`), ausencia de ejecución del cuerpo y `live_allocations=0`; la prueba también
  verifica que la rama `--native-threads` compile.

La batería queda en **6 pruebas diferenciales y 137 de integración verdes**. La siguiente
ampliación de concurrencia es introducir puntos seguros de cancelación y grupos nativos
para que `spawn_scope` pueda propagar cancelación sin detener hilos de forma insegura.

## 167. `yield()` como primitiva de coordinación — 2026-09-19

La biblioteca estándar expone `yield() -> Void` para que un programa pueda ceder
explícitamente el turno:

- en el intérprete y el backend C cooperativo ejecuta como máximo una tarea pendiente y
  conserva el orden determinista del scheduler;
- con `--native-threads` llama a la operación de cesión del sistema operativo (`Sleep(0)`
  en Windows y `sched_yield()` en POSIX), sin prometer fairness ni sincronización por sí
  misma;
- `examples/concurrency_yield.ostrin` y su prueba comparan `main`, `task`, `after`,
  verifican `live_allocations=0` y comprueban que el modo de hilos reales compila.

La batería queda en **6 pruebas diferenciales y 138 de integración verdes**. `yield()` es
también el punto de coordinación visible para programas mientras la cancelación por
puntos seguros y los grupos nativos siguen pendientes.

## 168. Runtime C cooperativo portable para la siguiente etapa WASI — 2026-09-19

El emisor C separa ahora la superficie de hilos reales de la que necesita el scheduler
cooperativo:

- solo `--native-threads` define `OSTRIN_NATIVE_THREADS` y activa `pthread`/Windows,
  `OstrinMutex`, condiciones y `OstrinThread` reales;
- el modo por defecto conserva los mismos nombres de runtime con tipos enteros y funciones
  no-op, de forma que heap, registro de tareas y `yield()` no requieren headers de threads;
- `native_backend` conserva pruebas de ejecución para ambos modos y una prueba de emisión
  comprueba que los headers quedan guardados por el macro correcto.

Esto no convierte todavía `ostrinc --compile` en un backend WASM: siguen pendientes el
toolchain C/WASI, la sustitución de APIs de proceso/archivos y el smoke test de un programa
Ostrin compilado a WASM. La batería queda en **6 pruebas diferenciales y 139 de integración
verdes**.

## 169. Compilación de programas hacia WASI — 2026-09-19

La ruta de distribución WASM dejó de cubrir únicamente al compilador. La CLI acepta ahora
`--target wasm32-wasi` junto con `--compile` o `--emit-c`:

- `--compile` selecciona `OSTRIN_WASI_CC` o `clang`, añade `--target=wasm32-wasi` y usa
  `OSTRIN_WASI_SYSROOT` cuando está definido;
- el nombre de salida por defecto es `<entrada>.wasm`, y `--native-threads` se rechaza para
  este target porque WASI todavía usa el runtime cooperativo sin pthreads;
- el workflow descarga una versión y SHA-256 fijados de wasi-sdk, compila `hello.ostrin`,
  ejecuta `hello.wasm` bajo Node WASI y empaqueta el programa junto con `ostrinc.wasm`.

La prueba unitaria de la CLI verifica la emisión cooperativa y la incompatibilidad explícita
con hilos nativos. La batería local queda en **6 pruebas diferenciales y 140 de integración
verdes**; la ejecución del toolchain WASI queda verificada por el workflow de GitHub.

## 170. Paquetes locales en el backend nativo y WASI — 2026-09-19

El proyecto de ejemplo con `ostrin.toml`, `entry = "main.ostrin"` y una dependencia relativa
`path = "../shared_lib"` ya no se valida únicamente con el intérprete:

- una prueba de integración compila el proyecto completo con `--compile --project` y ejecuta
  el binario nativo, comprobando que el import de `helpers.greet` conserva su salida;
- el workflow WASI produce además `pkg_project.wasm`, lo ejecuta bajo Node WASI y exige la
  salida `hola, Ostrin`;
- el artefacto de distribución contiene `ostrinc.wasm`, `hello.wasm` y `pkg_project.wasm`,
  todos incluidos en un único archivo de checksums.

Esto establece una primera garantía de que la selección de entrada y las dependencias locales
son compatibles con los dos backends compilados. Git y el registro remoto siguen fuera del
alcance: las dependencias Git continúan requiriendo clonación explícita y conversión a `path`.
La batería queda en **6 pruebas diferenciales y 141 de integración verdes**.

## 171. Cancelación cooperativa en puntos seguros — 2026-09-19

La cancelación de tareas ya no se limita a tareas que todavía no empezaron:

- `Task.cancel()` mantiene la transición inmediata `Pending → Cancelled`, pero en una tarea
  `Running` registra una solicitud y devuelve `true`; devuelve `false` únicamente para tareas
  terminadas o ya canceladas;
- el intérprete comprueba la solicitud en cada frontera de sentencia y conserva
  `TaskCancelled` como señal interna del scheduler, de modo que la tarea termina sin ejecutar
  su siguiente operación y `join()` conserva el error `task was cancelled`;
- el backend C cooperativo y el modo `--native-threads` usan un contexto de checkpoint basado
  en `setjmp`/`longjmp`. `yield()` comprueba la solicitud después de ceder el turno, restaura
  el contexto anidado y marca la tarea como cancelada; no hay preempción arbitraria de código
  que no alcance un punto seguro;
- `examples/concurrency_cancel_safe.ostrin` cubre una tarea que imprime `started`, cede,
  recibe una cancelación desde otra tarea y no llega a imprimir `must-not-run`. La prueba
  compara intérprete y C cooperativo, verifica `live_allocations=0` y comprueba que el modo
  de hilos reales emite el checkpoint.

La batería queda en **6 pruebas diferenciales y 142 de integración verdes**. La siguiente
brecha de concurrencia es completar la propagación y administración de grupos en `spawn_scope`;
la cancelación sigue siendo cooperativa y no interrumpe tareas bloqueadas sin un checkpoint.

## 172. Ownership de entornos capturados en tareas — 2026-09-19

La cancelación cooperativa reveló una ruta de ownership que no podía depender del cuerpo de
la tarea: un `longjmp` desde `yield()` puede saltar por encima del cleanup que normalmente se
emite al final del callback. El backend nativo ahora:

- genera un destructor separado para cada entorno capturado, que libera sus referencias hijas
  y el propio entorno;
- guarda ese destructor en el header de `Task<T>` y lo ejecuta desde el wrapper del runtime
  después de un callback normal o cancelado;
- usa el mismo destructor desde el destructor de la tarea cuando una tarea pendiente se
  cancela y se descarta sin llegar a ejecutarse;
- aplica el contrato a los dos modos de tareas y lo protege con una prueba que cancela una
  tarea que captura una `List`, además del `live_allocations=0` y la comparación
  intérprete↔nativo ya existentes.

La batería se mantiene en **6 pruebas diferenciales y 142 de integración verdes**. Esto
reduce la deuda de ownership del backend nativo, pero la inserción general de RC por último
uso sobre la IR, ciclos y escapes complejos todavía queda pendiente.

## 173. Enlace de libm en nativo Unix — 2026-09-19

La matriz de GitHub estaba roja únicamente en Ubuntu, aunque Windows y macOS pasaban. La
reproducción en Ubuntu 24.04 aisló tres pruebas de paquetes (`autodiff`, `plot` y `tables`):
el código C generado usaba `sqrt` o `round`, pero el comando de enlace no añadía `libm`, por
lo que `cc` terminaba con referencias indefinidas. El driver nativo ahora añade `-lm` en
targets Unix y conserva el enlace anterior en Windows; la suite Linux local vuelve a cubrir
los mismos 142 ejemplos sin esa diferencia de plataforma.

## 174. Grupos estructurados y cancelación propagada — 2026-09-19

`spawn_scope` deja de ser únicamente una marca ordinal del scheduler y pasa a tener grupos
explícitos en los tres caminos de ejecución:

- el intérprete asocia cada tarea con su grupo y registra los scopes activos de la tarea;
  cancelar una tarea `Running` marca también sus grupos anidados, cancela hijos pendientes
  y permite que los hijos activos observen `TaskCancelled` en sus fronteras;
- el runtime C mantiene `OstrinTaskGroup` y frames anidados por ejecución, propaga una
  cancelación a los grupos activos del padre y drena sus nodos antes de liberar los frames;
  el registro usa adaptadores de cancelación tipados, evitando conversiones incompatibles
  de punteros de función en GCC;
- la espera de `select` comprueba la cancelación después de liberar su lista temporal de
  canales, evitando que el `longjmp` salte el cleanup y deje referencias vivas; los handles
  creados dentro de callbacks cancelados se limpian junto con el entorno capturado;
- `examples/concurrency_scope_cancel.ostrin` comprueba la propagación padre→hijo, el orden
  observable y `live_allocations=0` en intérprete, C cooperativo y `--native-threads`.

La prueba se ejecutó cuatro veces para descartar la carrera nativa y después la suite
completa quedó en **6 pruebas diferenciales y 143 de integración verdes**. La brecha
restante de concurrencia es despertar de forma cancelable una E/S bloqueante; el backend
WASM/distribución y la bitácora de cada bloque continúan formando parte de la ruta de
producción.

## 175. Recepción de canal cancelable en hilos nativos — 2026-09-19

La espera bloqueante de `Channel<T>.receive()` ya no puede dejar una tarea nativa dormida
indefinidamente después de que otra tarea solicite su cancelación:

- Windows usa una espera de condición con timeout corto y POSIX usa
  `pthread_cond_timedwait`; ambos caminos conservan la señalización inmediata de
  `send`/`close`, pero también vuelven periódicamente al runtime.
- El receptor libera el mutex del canal antes de llamar a
  `ostrin_task_checkpoint()`. Si la tarea o su grupo fueron cancelados, el checkpoint
  puede salir del callback sin saltarse una sección crítica ni dejar el canal bloqueado.
- El intérprete comprueba la cancelación al entrar y después de ejecutar una tarea
  pendiente mientras espera una recepción o itera un canal, manteniendo la misma
  semántica observable.
- `examples/concurrency_cancel_blocked_receive.ostrin` cubre `receive()` vacío,
  cancelación desde otra tarea, ausencia de ejecución posterior y
  `live_allocations=0` en el intérprete, C cooperativo y `--native-threads`; la prueba
  también exige que el C emitido contenga el timeout y el checkpoint.

La batería queda en **6 pruebas diferenciales y 144 de integración verdes** después de
esta ampliación. La cancelación de E/S externa arbitraria, como archivos o red, sigue
requiriendo un runtime de I/O cooperativo; no se pretende preemptar llamadas del sistema
que no regresan al runtime.

## 176. Ownership idempotente de handles de tareas — 2026-09-20

La limpieza de cancelación del backend C deja de asumir que todos los handles registrados
siguen poseyendo una referencia local:

- cada nodo temporal de `OstrinTaskExecution.owned_handles` conserva una marca `released`;
  los cleanup normales y `drop()` pasan por `ostrin_release_owned()`, que marca el nodo
  antes de liberar la referencia local;
- si una cancelación salta por encima del cleanup léxico, el drenado solo libera los
  handles que aún no habían sido soltados, mientras que la referencia del registro de
  tareas continúa separada y válida;
- las liberaciones internas del registro, destructores de entornos y destructores de
  colecciones siguen usando `ostrin_release()` y no consumen por accidente la marca de
  una referencia local distinta;
- `examples/concurrency_cancel_after_drop.ostrin` fuerza un hijo creado dentro de un
  `spawn_scope`, descarta explícitamente su handle y cancela el padre mientras espera un
  acuse. La prueba compara intérprete, C cooperativo y `--native-threads`, exige
  `live_allocations=0` y verifica la nueva rutina en el C emitido.

La batería queda en **6 pruebas diferenciales y 145 de integración verdes**. Esto cierra
una ruta concreta de doble liberación durante cancelación, pero el ownership general sobre
la IR, los escapes complejos y los ciclos siguen siendo trabajo pendiente.

## 177. HIR nativo para escalares científicos y enteros de ancho fijo — 2026-09-20

El backend nativo deja de abandonar al AST funciones HIR que usan `Float32` o enteros
fijos cuando el resto de su cuerpo ya es representable:

- `hir_c.rs` reconoce `Float32`, `Int8`/`Int16`/`Int32` y `UInt8`/`UInt16`/`UInt32`/`UInt64`
  en firmas, literales, records, impresión y conversiones hacia flotantes;
- las operaciones binarias de enteros fijos conservan las comprobaciones de overflow y
  división por cero del backend AST, en vez de aceptar el wrap silencioso de C;
- las operaciones `Float32` mantienen el redondeo a `float` y la comparación nativa,
  mientras que `to_string()` usa las mismas rutinas de formato del runtime;
- el trinquete de generación HIR sube de 115 a **127 funciones** y la prueba diferencial
  conserva cero divergencias entre checker, intérprete y backend nativo.

La batería local queda en **6 pruebas diferenciales y 146 de integración verdes**. El
fallback AST continúa siendo necesario para cantidades, arrays y otras familias complejas;
la siguiente deuda estructural sigue siendo hacer que la IR de ownership sea consumida por
el backend, no solo inspeccionada.

## 178. Primer emisor C consumiendo la IR explícita — 2026-09-20

La IR deja de ser únicamente una representación observable y pasa a alimentar una familia
real del backend nativo. `compiler/src/ir_c.rs` consume funciones HIR→IR de un solo bloque
con valores escalares, materializa sus temporales SSA como temporales C y conserva las
operaciones propias del runtime, incluida la división entera y la impresión numérica.

La integración en `codegen.rs` mantiene una cadena de fallback medible: IR primero, HIR
después y AST al final. `--native-type-report` distingue ahora `ir-generated` de
`hir-generated`; el trinquete diferencial suma ambas rutas para que migrar una función de
HIR a IR cuente como avance sin ocultar regresiones. La prueba
`native_ir_emitter_handles_scalar_functions` compara `int_division.ostrin` en intérprete y
binario nativo, y la suite completa conserva cero divergencias.

La frontera es deliberadamente conservadora: cualquier temporal gestionado, bloque de
control aún no emitido o entero de ancho fijo cae al camino HIR/AST, porque la IR todavía no
tiene intrínsecos de overflow comprobado ni lowering completo de `retain/release`. Esto
evita que la migración cambie la semántica; el siguiente paso es extender el emisor a
control de flujo y luego hacer que consuma de verdad la IR de ownership.

La batería queda en **6 pruebas diferenciales y 147 de integración verdes**.

## 179. CFG escalar nativo desde la IR — 2026-09-20

La emisión C desde IR deja de estar limitada a funciones lineales. `ir_c.rs` ahora consume
CFGs escalares completos con etiquetas y saltos explícitos: ramas, `if` anidados, llamadas
recursivas, bucles `while` y selección de valores mediante `phi` usan temporales SSA
declarados en el marco C y un registro del bloque predecesor.

La bajada HIR→IR también se hizo semánticamente correcta para este bloque:

- los `while` crean `phi` para los bindings visibles que sobreviven entre iteraciones y
  actualizan sus entradas con el valor producido por el backedge;
- `if` anidados y handlers de `try` registran el bloque que realmente conecta con el merge,
  no el bloque sintáctico de entrada;
- el verificador de IR calcula predecesores y rechaza `phi` vacíos, duplicados, incompletos o
  que apunten a bloques que no son aristas CFG reales.

`examples/native_ir_control_flow.ostrin` prueba una clasificación con ramas anidadas y una
suma con estado de bucle. La salida del intérprete y del binario nativo coincide (`-1`, `0`,
`1`, `15`), mientras que valores gestionados, iteradores, `match` y aritmética de enteros de
ancho fijo permanecen en el fallback verificado hasta tener sus contratos completos.

La batería queda en **6 pruebas diferenciales y 148 de integración verdes**. La siguiente
frontera es conectar ownership/último uso de la IR a este emisor sin perder el contrato de
destructores y `retain/release` del backend actual.

## 180. Aritmética de ancho fijo comprobada desde la IR — 2026-09-20

La familia escalar del emisor IR ya no necesita abandonar al HIR para los enteros de ancho
fijo. `ir_c.rs` conserva las mismas reglas que los emisores anteriores:

- `Int8`/`Int16`/`Int32` y `UInt8`/`UInt16`/`UInt32`/`UInt64` usan
  `__builtin_add_overflow`, `__builtin_sub_overflow` y `__builtin_mul_overflow` sobre el
  tipo C exacto;
- la división comprueba cero y el caso `min / -1` con un cociente temporal `__int128`;
- la negación con signo rechaza `min` antes de calcular, y las comparaciones, constantes y
  `print` mantienen el tipo y el formato del intérprete;
- tipos fijos incompatibles u operaciones no soportadas siguen provocando el fallback
  verificable, no una conversión silenciosa a C con wraparound.

`examples/native_ir_sized.ostrin` cubre llamadas entre funciones escalares, suma `UInt8`,
división y negación `Int32`; el binario nativo produce `120`, `-4`, `-7`, igual que el
intérprete. La prueba existente de overflow continúa fallando con el mismo diagnóstico y
ahora también atraviesa el camino IR cuando la función es elegible. El trinquete sube de
127 a **131 funciones HIR/IR** y la batería queda en **6 pruebas diferenciales y 149 de
integración verdes**.

La deuda siguiente sigue siendo ownership/último uso sobre la IR: los valores gestionados,
iteradores y releases alrededor de `phi`, loops, scopes y escapes aún deben migrarse sin
romper destructores ni `retain/release`.

## 181. Sitio y documentación pública sincronizados — 2026-09-20

La superficie pública de GitHub Pages se actualizó para reflejar el estado real del
repositorio, no una fotografía anterior del prototipo:

- `website/index.html`, `examples.html`, `ecosystem.html` y `roadmap.html` muestran los
  **155** programas `.ostrin` y las **149 pruebas de integración + 6 diferenciales**;
- `website/docs.html` enlaza ahora el archivo completo de **21 documentos**, incluido el
  diseño de distribución/WASI, HIR/IR y la jerarquía numérica;
- se retiró el parche de JavaScript que corregía en tiempo de ejecución el texto antiguo de
  “seventeen documents”; el contenido fuente queda correcto y accesible sin JavaScript;
- el sitio mantiene explícita la frontera de producto: paquetes locales y artefactos WASI
  existen, mientras que el registro público, instaladores de release y playground de
  navegador siguen en construcción.

README y sitio quedan alineados con `ESTADO_Y_PLAN.md`; la suite de código no cambia porque
este bloque es documental y de presentación.

## 182. Dependencias Git opt-in y lockfile resoluble — 2026-09-20

El sistema de paquetes deja de reconocer las dependencias Git únicamente para rechazarlas:

- una compilación normal sigue siendo offline y conserva el diagnóstico claro para un
  `git = ...` sin autorización explícita;
- `ostrinc --fetch --project DIR` clona o actualiza cada dependencia en
  `DIR/.ostrin/packages/`, usando una clave estable derivada del alias, URL y ratchet;
- el checkout se separa (`detached HEAD`) en el `tag` o `rev` pedido y el lockfile registra
  `source`, URL, ratchet solicitado, commit resuelto, versión del paquete y ruta portable;
- el cache local queda ignorado por Git y la prueba de integración usa un repositorio local
  temporal para verificar el flujo completo sin depender de un servicio externo.

La batería queda en **6 pruebas diferenciales y 150 de integración verdes**. La verificación
criptográfica del contenido del checkout y la resolución transitiva siguen pendientes; el
commit fijado en `ostrin.lock` ya hace explícita la identidad de la revisión usada.

## 183. El lockfile gobierna la resolución — 2026-09-20

La primera implementación de `--fetch` ya no deja el lockfile como un artefacto meramente
informativo:

- `ostrinc` lee `ostrin.lock` antes de resolver dependencias y rechaza entradas extra,
  fuentes cambiadas, rutas path divergentes, versiones modificadas y URLs o ratchets Git
  distintos del manifiesto;
- una entrada Git válida se reutiliza desde la caché y se comprueba con
  `git rev-parse HEAD`, sin ejecutar `fetch` ni tocar la red durante una build normal;
- `--locked` exige un lockfile completo y una caché válida, y no escribe ningún lockfile;
- `--fetch` puede restaurar una caché Git ausente o desactualizada, tras lo cual vuelve a
  registrar la revisión resultante.

La prueba de integración deja el clon sin remoto, ejecuta una build normal y otra `--locked`,
comprueba que el lockfile no cambia, verifica el fallo ante una caché ausente y finalmente
comprueba su restauración con `--fetch`. Hash de contenido y resolución transitiva siguen siendo
los siguientes huecos del sistema de paquetes.

## 184. `String` gestionado desde la IR — 2026-09-20

La primera familia de valores gestionados deja de depender exclusivamente del fallback HIR/AST
para llegar al backend nativo. La bajada HIR→IR conserva ahora el contenido fuente de los
literales `String` y el emisor C aplica el escape común de `codegen`, de modo que las comillas,
backslashes y caracteres de control no dependen del formato de depuración de Rust.

`ir_c.rs` genera C para `String` en constantes, parámetros, llamadas, concatenación,
igualdad/desigualdad con `strcmp`, `print`, ramas y `phi`. La pasada de ownership clasifica
`String` como tipo gestionado, consume sus marcadores `retain`/`release` y trata dos casos de
transferencia de forma explícita: un `phi` que retorna directamente conserva la propiedad para
el caller, mientras que un alias usado por `print` retiene el destino y libera la referencia
fuente en el bloque predecesor.

`examples/native_ir_strings.ostrin` y `native_ir_emitter_handles_strings_and_ownership_markers`
comparan intérprete y binario nativo, exigen que el informe use al menos seis funciones IR,
inspeccionan los marcadores generados y ejecutan `--leak-check` con `live_allocations=0`. La
suite queda en **6 pruebas diferenciales y 151 de integración verdes**.

El siguiente frente de memoria sigue siendo extender este contrato a records, colecciones,
loops, scopes y escapes complejos; los ciclos y la destrucción completa de agregados no se
consideran cerrados por esta migración.

## 185. Núcleo de `List<T>` consumido desde la IR — 2026-09-20

La segunda familia gestionada que llega al emisor IR es el núcleo de listas con elementos
escalares (`Int`, `Float`, `Bool`, `String` y enteros de ancho fijo). `ir_c.rs` ya materializa
el tipo `List_<T>*` que generan los helpers del backend y consume estas operaciones:

- literales de lista y listas vacías mediante `List_<T>_new_from_array`;
- parámetros prestados y llamadas a funciones que reciben/devuelven listas;
- indexación, `length`/`count`, `push` y `remove_at`;
- `List<String>` con elementos gestionados, cuyos helpers retienen al almacenar y transfieren
  ownership al extraer un elemento.

La pasada de ownership distingue parámetros prestados de valores producidos localmente, libera
listas en su último uso y no duplica los `retain` de elementos que ya ejecutan los constructores
o mutadores nativos. Para un `Aggregate` de lista el lowering conserva el release del valor
fuente, mientras el helper C conserva la referencia almacenada; esto evita la fuga encontrada
durante la prueba de `List<String>`.

`examples/native_ir_lists.ostrin` y `native_ir_emitter_handles_lists_and_ownership_markers`
comparan intérprete y nativo, exigen cuatro funciones IR, verifican los helpers emitidos y
ejecutan `--leak-check` con `live_allocations=0`. La suite queda en **6 pruebas diferenciales y
152 de integración verdes**.

Mapas, sets, combinadores, listas de records y ownership de llamadas que transfieren alias
siguen deliberadamente en el fallback verificado; son el siguiente bloque de colecciones.

## 186. Núcleo escalar de `Map`/`Set` consumido desde la IR — 2026-09-20

La siguiente porción migra a `ir_c.rs` las colecciones hash cuyos elementos son escalares
(`Int`, `Float`, `Float32`, `Bool`, `String` o enteros de ancho fijo). El emisor C ya
materializa los tipos monomorfizados `Map_<K>_<V>*` y `Set_<T>*` que genera el runtime y
consume estas operaciones:

- literales y colecciones vacías mediante los constructores nativos;
- parámetros prestados y llamadas a funciones que reciben mapas o conjuntos;
- `set`, `add`, `remove`, `contains_key`, `contains` y `count`;
- `keys()` y `values()`, cuyos resultados vuelven al núcleo de listas escalares.

Los helpers de hash retienen las claves y valores gestionados al almacenarlos, por lo que el
lowering de ownership no inserta un `retain` duplicado antes de un agregado. Los releases de
los valores fuente se colocan después de los agregados y métodos de transferencia; los
destructores tipados de `Map`/`Set` liberan sus buffers y sus strings hijas. La prueba
`examples/native_ir_maps_sets.ostrin` compara intérprete y binario nativo, exige que los tres
cuerpos se emitan desde la IR y comprueba `--leak-check` con `live_allocations=0`. La suite
queda en **6 pruebas diferenciales y 153 de integración verdes**.

Los lookups `get`/`remove` que devuelven `Option`, listas de records y valores compuestos aún
conservan el fallback HIR/AST; migrar la representación estructural de `Option` a esta misma
IR es el siguiente paso.

## 187. `Option` escalar y lookups de `Map` desde la IR — 2026-09-20

La representación de `Option<T>` generada por el backend ya está cerrada como un struct C por
valor (`has` más `value`). El siguiente bloque la conectó con la IR para payloads escalares no
gestionados: `Int`, `Float`, `Float32`, enteros de ancho fijo y `Bool`.

`ir_c.rs` ahora emite `None` como un `Option_<T>` con `has = false`, `Some(x)` como un
compound literal con `has = true`, y los métodos `is_some`, `is_none`, `unwrap` y `unwrap_or`
como expresiones C sobre ese struct. Al ser un valor escalar, `Option<T>` no recibe
`retain/release`; el análisis de ownership lo excluye de las familias gestionadas. `Option`
con `String`, records, colecciones u otros payloads gestionados conserva el fallback hasta que
se defina su contrato de copia y destrucción.

El mismo emisor consume ahora `Map_<K>_<V>_get` y `Map_<K>_<V>_remove` cuando `V` es escalar,
por lo que `Map<String, Int>.get/remove` devuelven directamente `Option_Int` nativo. Las
claves siguen siendo préstamos al helper y la pasada de último uso libera las claves de
`String` en los puntos seguros, mientras el mapa mantiene la propiedad de sus entradas.

`examples/native_ir_map_options.ostrin` cubre consultas presentes y ausentes, `unwrap_or`,
`is_some`, `is_none`, `unwrap`, llamadas a funciones que retornan `Option<Int>` y la extracción
con `Map.remove`. `native_ir_emitter_handles_scalar_map_options` exige tres funciones IR,
inspecciona `Option_Int` y los helpers de mapa en el C generado, compara nueve líneas entre
intérprete y binario nativo y comprueba `live_allocations=0`. La suite queda en **6 pruebas
diferenciales y 154 de integración verdes**.

La siguiente frontera sigue siendo `Option` con payload gestionado, patrones sobre `Option`,
listas de records y llamadas que transfieren ownership de forma no lineal.

## 188. `Option<String>` y patrones `Some`/`None` desde la IR — 2026-09-20

El bloque siguiente amplió la representación anterior sin convertir todos los agregados en
casos especiales. `Option<String>` usa el mismo struct C por valor (`has` más puntero `value`),
pero ahora la IR conoce que su payload participa en el conteo de referencias:

- `Some(text)` retiene el string al construir el `Option`; el último uso del argumento libera
  la referencia original, de modo que el `Option` conserva una referencia propia.
- `Map.get` retiene el valor gestionado que copia al `Option`; `Map.remove` transfiere la
  referencia que antes pertenecía al mapa. El destructor del mapa sigue siendo responsable
  únicamente de las entradas que permanecen en él.
- `Retain` y `Release` sobre `Option<String>` comprueban `has` y aplican la operación al
  puntero solo cuando existe payload. `Option` escalar continúa siendo una copia por valor y
  no genera llamadas al runtime.

La bajada HIR→IR ahora tipa los binds de `Some(name)` y omite el falso bind que antes creaba
`None` como si fuera una variable. `ir_c.rs` consume `PatternTest` para `Some`/`None` y
`PatternBind` para el payload, por lo que `match` simple sobre `Option<String>` puede emitirse
desde CFG/SSA. Las funciones genéricas no se anuncian como llamadas directas del emisor IR:
si necesitan monomorfización, conservan el camino HIR que registra la instancia C correcta.

`examples/native_ir_managed_options.ostrin` usa strings concatenados dinámicamente, funciones
que producen/consumen `Option<String>`, un `match`, `Map<String,String>.get/remove`,
`unwrap_or`, `is_none` y `unwrap`. `native_ir_emitter_handles_managed_options_and_patterns`
compara siete líneas entre intérprete y binario, comprueba `Option_String`, los helpers de mapa,
los retains condicionales y termina con `live_allocations=0`. La suite queda en **6 pruebas
diferenciales y 155 de integración verdes**.

Quedan fuera de esta porción `Option` de records, listas, mapas, sets o payloads anidados,
patrones anidados y el análisis completo de llamadas que almacenan o devuelven aliases
gestionados de forma no lineal.

## 189. Records concretos y `Option<Record>` desde la IR — 2026-09-20

El siguiente bloque llevó al emisor C de la IR la representación que el backend ya tenía
para records concretos: punteros con identidad, callbacks `ostrin_drop_<Record>` y campos
que pueden contener otros records o referencias gestionadas. `Aggregate` conserva ahora los
nombres de campos, de modo que la construcción desde CFG no depende del orden accidental del
literal; el emisor valida el conjunto de campos y asigna cada valor sobre la estructura C
registrada.

También se habilitó `Option<Record>` como payload gestionado:

- `Some(record)` retiene el puntero y el último uso libera la referencia original; `None`,
  `PatternTest` y `PatternBind` usan el struct `Option_<Record>` por valor.
- Los accesos a campos retienen aliases prestados y liberan el alias cuando el campo ya no
  vuelve a usarse; los records y opciones reciben `retain/release` condicionales desde la IR.
- Las funciones no genéricas que reciben o devuelven records pueden permanecer enteramente
  en IR; enums, records genéricos, patrones anidados y escapes no lineales conservan el
  fallback HIR/AST verificado.

`examples/native_ir_records.ostrin` cubre records anidados, strings dentro de records,
`Option<Record>`, patrones `Some`/`None`, llamadas entre funciones y extracción de campos.
`native_ir_emitter_handles_records_and_option_record_ownership` compara cuatro líneas entre
intérprete y binario nativo, exige cinco funciones IR, inspecciona los destructores y termina
con `live_allocations=0`. La suite queda en **6 pruebas diferenciales y 156 de integración
verdes**.

La siguiente frontera es completar el análisis de ownership en joins/loops/scopes y extender
la misma representación a otros payloads gestionados de `Option`, colecciones de records y
patrones anidados.

## 190. Ownership de `Phi` en joins y bucles — 2026-09-20

La pasada `--ownership-ir` deja de tratar todos los `Phi` gestionados como aliases que deben
retenerse. Cuando cada entrada gestionada solo llega al `Phi` (salvo los `StoreLocal` no-op que
conserva la construcción SSA), la entrada transfiere su referencia al resultado del join. Esto
evita liberar una entrada antes de que el bloque de convergencia pueda leerla y elimina el
`retain` duplicado de ramas como `if` que producen un `String` dinámico.

Para `Phi` de bucle, el lowering reconoce un backedge alcanzable desde el bloque de convergencia.
Si el valor corriente tiene su último uso seguro en el bloque que vuelve al encabezado, inserta
el `release` después de ese uso. La salida del bucle puede conservar la referencia y retornarla,
por lo que el caso mantiene la transferencia normal al caller sin liberar en la rama de salida.
El análisis sigue siendo deliberadamente conservador para scopes anidados, escapes, llamadas
transferentes y uses repartidos por CFGs más complejos.

`examples/native_ir_managed_loop.ostrin` concatena una cadena en un `while` nativo tres veces;
la prueba diferencial `native_ir_emitter_releases_managed_loop_phi_values` verifica `startxxx`,
la presencia de releases en la IR y `live_allocations=0` en el binario. La suite queda en
**6 pruebas diferenciales y 157 de integración verdes**.

## 191. `List<Record>` desde la IR — 2026-09-20

El emisor C de la IR acepta ahora listas cuyo elemento es un record concreto: tipos
`List_<Record>*`, construcción con `new_from_array`, indexado, `push`, `remove_at`,
`length`/`count` y vacíos tipados, reutilizando los helpers y destructores que el backend ya
registra para el record. El ownership por tipo existente (retain de aliases prestados,
transferencia en `remove_at`) se aplica sin cambios. Maps y sets siguen limitados a escalares.

`examples/native_ir_record_lists.ostrin` mueve las tres funciones al camino IR (antes solo
`make`); `native_ir_emitter_handles_record_lists` compara intérprete y binario y exige
`live_allocations=0`. Suite: **6 diferenciales y 158 de integración**.

Siguiente frontera: `Option<List>`/listas anidadas, patrones anidados y bucles `for` sobre listas.

## 192. `for` sobre listas como bucle SSA en la IR — 2026-09-20

`lower_for` reconoce iteradores `List<T>` y genera un bucle con `Phi` de índice, `length`,
`Index` y `+ 1`, en lugar del iterador opaco que el emisor C no sabía traducir. Los bucles
con `break`/`continue` conservan la forma anterior (siguen en el fallback) porque sus
aristas no aportan entradas de `Phi` correctas.

Además, `while` y `for` solo crean `Phi` de bucle para variables realmente asignadas en el
cuerpo (`assigned_in`). Antes, cada variable visible —incluida la lista iterada— recibía un
`Phi` cuyo `retain` de cabecera no se liberaba en la salida, filtrando una referencia por
bucle. `examples/native_ir_for_lists.ostrin` (suma de `List<Int>` y concatenación de
`List<String>`) termina con `live_allocations=0`. El test de colecciones HIR cuenta ahora
funciones HIR o IR porque su función pasó a la IR. Suite: **6 diferenciales y 159 de integración**.

Siguiente: `break`/`continue` con `Phi` correcto, `for` sobre rangos y `Option<List>`.

## 193. Formateador oficial `--fmt` — 2026-09-20

Nuevo módulo `fmt.rs` y flags `--fmt` (stdout), `--fmt --write` y `--fmt --check`. Es un
formateador de *layout*: sangría por profundidad de `{ ( [`, recorte de espacios finales,
colapso de líneas en blanco (ninguna tras `{` ni antes de `}`), fin de línea LF y salto
final. Comentarios de bloque y strings multilínea se conservan tal cual. Antes de devolver
el texto, `format_source` compara el flujo de tokens del lexer antes y después y se niega
a formatear si difiere, de modo que un fallo del formateador nunca cambia el programa.

Verificado sobre los 154 ejemplos del repositorio (todos formatean, idempotente; solo 2
necesitaban cambios), con 2 pruebas unitarias y `fmt_normalizes_layout_and_supports_write_and_check`.
Suite: **6 diferenciales y 160 de integración** (+2 unitarias del binario).

Siguiente en D: `ostrinc test` como subcomando, docs generadas, CI multiplataforma de release.

## 194. `%`, `break`/`continue` con `Phi` y fusión de ramas en la IR — 2026-09-20

**Operador `%`.** Nuevo `BinOp::Rem` (parser, checker E1041 para tipos inválidos, intérprete,
codegen AST, HIR→C e IR→C). `Int % Int` trunca como C (`ostrin_irem`, `x % -1 == 0`),
`Float`/`Float32` usan `fmod`/`fmodf`, los enteros de ancho fijo comprueban divisor cero y usan
`__int128`. Dividir por cero es error de ejecución en ambos backends.

**Corrección de la IR.** Al probar `break`/`continue` apareció un fallo latente: `lower_if` y
`lower_match` no fusionaban las variables asignadas en las ramas (`if c { r = 5 }; r` devolvía
el valor anterior). No se manifestaba porque el `Phi` de tipo `Void` hacía que `ir_c` rechazara
esas funciones. Ahora ambos crean un `Phi` por variable reasignada (`merge_branch_bindings`), el
resultado `Void` es una constante unit, `match` restaura los bindings entre brazos, y el orden
de los `Phi` de bucle es determinista (nombres ordenados).

**`break`/`continue`.** Cada bucle registra sus aristas (`LoopEdges`): los `continue` aportan
entradas a los `Phi` de cabecera (`while`) o de un bloque `step` (`for` sobre lista, solo si
el cuerpo contiene `continue`; si no, el incremento queda en línea para que el pase de
ownership vea el backedge donde están los últimos usos), y los `break` aportan un `Phi` de
salida por variable asignada.

**Límites de ownership (compuertas).** El pase lineal solo libera valores gestionados usados en
un único bloque. Por eso: (1) una fusión gestionada en `if`/`match`/`break`/`continue` marca la
función con un `Opaque("managed_join")` y (2) iterar con `for` una lista propia (no parámetro)
marca `owned_loop_source`; en ambos casos la función sigue por el camino HIR verificado. Además
el HIR liberaba mal: `for x in <temporal>` nunca liberaba la lista y la reasignación de un local
gestionado en un ámbito anidado no liberaba el valor viejo; ambos casos quedan corregidos.

Ejemplos `native_ir_branch_merge`, `native_ir_break_continue`, `rem_operator`; la prueba
`native_ir_merges_branch_and_loop_bindings_and_supports_rem` compara intérprete y binario con
`live_allocations=0`. Suite: **6 diferenciales, 162 de integración y 2 unitarias**.

Siguiente: liveness entre bloques en el pase de ownership (liberación en aristas) para quitar
las compuertas (1) y (2); después `for` sobre rangos.

## 195. Workflow de release multiplataforma — 2026-09-20

`.github/workflows/release.yml`: al empujar una etiqueta `v*` (o lanzarlo a mano) compila y
prueba `ostrinc` en Linux x86_64, macOS arm64 y Windows x64, empaqueta el binario con
`LICENSE`, `README.md` y `examples/`, genera `*.sha256` y, solo con etiqueta, publica una
release con `gh release create --generate-notes`. No se ha publicado ninguna release desde
esta sesión; el workflow queda listo para la primera etiqueta que decida el mantenedor.

## 196. Liberación entre bloques, métodos de `String` en IR y `and`/`or` con cortocircuito — 2026-09-20

**Ownership entre bloques.** `release_points_at_returns` (en `ownership.rs`) ya libera valores
gestionados propios usados en varios bloques: se sueltan antes de cada `Return` cuando su bloque
de definición domina todas las salidas y no está en un ciclo; una salida que devuelve el propio
valor lo transfiere y no se libera. Se excluyen valores que entran en `Phi`/`Opaque`, regiones
(`RegionReturn`) y definiciones dentro de bucles. Con ello se elimina la compuerta
`owned_loop_source`: `for x in <lista propia>` vuelve a la IR sin fugas.

**Métodos de `String` en la IR** (`length is_empty trim to_upper to_lower contains starts_with
ends_with replace`), con los mismos helpers de `strings_runtime.c`.

**Corrección de semántica.** `and`/`or` evaluaban siempre ambos lados en el intérprete y en la IR
(`x != 0 and 10 / x > 1` abortaba). El intérprete corta cuando el lado izquierdo es `Bool`
(las máscaras `Array<Bool>` siguen siendo elemento a elemento) y la IR baja a control de flujo con
un `Phi`. El camino AST liberaba los locales *antes* de evaluar `return <expr>` de tipo escalar
(uso tras liberar); ahora evalúa a un temporal primero.

Ejemplos `native_ir_string_methods`, `native_ir_cross_block_ownership` (7 funciones, 0 fugas) y
`short_circuit`; prueba `native_ir_string_methods_cross_block_ownership_and_short_circuit`.
Suite: **6 diferenciales, 163 de integración y 2 unitarias**.

Frontera conocida: el camino AST sigue fugando los temporales al construir listas con valores
dinámicos (`[a + b]`) cuando la función no puede ir por IR; se cierra al migrar las familias
restantes a HIR/IR.

## 197. Ownership por liveness con `Phi`, retornos de parámetros y fuzzing diferencial — 2026-09-20

**Fuzzing diferencial.** `generated_programs_agree_between_interpreter_and_native_backend` genera
programas deterministas (aritmética, `%`, `if`/`else`, `while`/`for`, `break`/`continue`,
`and`/`or`, un `String` y una `List<Int>` por función) y compara intérprete y binario nativo,
exigiendo además `live_allocations=0`. Por defecto 4 semillas; `OSTRIN_FUZZ_SEEDS=N` la amplía
(120 semillas × 10 funciones verdes en esta sesión). Encontró: fugas de listas temporales
definidas en una sola ruta, bucles con strings reasignados y el fallo siguiente.

**Bug de seguridad de memoria corregido.** Un parámetro gestionado devuelto directamente
(`fn ident(s: String) -> String { s }`, o un `Phi` de parámetros) se devolvía sin retener: el
llamador liberaba un valor que no era suyo (uso tras liberar; el segundo `print` mostraba
basura). Ahora los parámetros ganan un `retain` al devolverse y en cada arista `Phi`.

**Pase de ownership reescrito sobre liveness.** `plan_cross_block_releases` calcula la vida de
cada valor gestionado en el CFG y decide: liberar tras el último uso en cada bloque donde muere,
`release` en la arista cuando el destino ya no lo necesita, o un `retain` por cada `Phi` que lo
consume mientras sigue vivo (si muere en la arista, la referencia se transfiere al `Phi`). Las
aristas críticas se dividen con un bloque nuevo (`apply_edge_ops`). Se eliminaron las heurísticas
antiguas de `Phi` y la compuerta `managed_join` del constructor de IR: `if`/`match`/`break`/
`continue`/bucles con strings, listas y records ya se compilan desde IR sin fugas. Una función con
un valor propio que el pase no puede resolver (`unresolved_functions`, visible en
`--ownership-ir`) cae al camino HIR verificado; los envoltorios `Option`/`Result` y los literales
quedan exentos.

Suite: **6 diferenciales, 164 de integración y 2 unitarias**.

Pendiente: `unwrap`/`unwrap_or` y otros consumidores de `Option` sin modelar en el pase; métodos
de `String` restantes (`split`, `lines`, `to_int`, `to_float`) en HIR/IR; ampliar el generador con
records, `Option` y closures.

## 198. Robustez del front end: parser exponencial corregido y fuzzing por mutación — 2026-09-20

`front_end_never_panics_on_mutated_sources` toma los ejemplos reales, borra/duplica/trunca/
intercambia tramos y exige que `--check` termine con diagnóstico (código 0 o 1), nunca con
pánico (101) ni desbordamiento de pila. `deeply_nested_input_does_not_crash_the_front_end`
cubre 3 000 paréntesis, 1 500 `if` anidados y corchetes sin cerrar.

Hallazgo: 1 500 `if` anidados colgaban el compilador. `parse_statement` intentaba parsear cada
sentencia como objetivo de asignación (`a.b = v`), la descartaba y la reparseaba como expresión,
de modo que cada nivel de anidamiento duplicaba el trabajo (2ⁿ; 20 niveles ≈ 3,5 s). Ahora hay un
único parseo y se decide por el `=` posterior. Con `OSTRIN_FUZZ_SEEDS=25` se ejecutaron unas
3 700 mutaciones sin pánicos. Suite: **6 diferenciales, 166 de integración y 2 unitarias**.

## 199. Biblioteca estándar en Ostrin (`std`) y correcciones del lenguaje — 2026-09-20

`compiler/std/{math,lists,strings}.ostrin` se embeben con `include_str!` y se resuelven como el
módulo virtual `<ostrin-std>` cuando el primer segmento del import es `std` (salvo que el proyecto
declare una dependencia `std`). Detalle en `docs/design/22-biblioteca-estandar.md`. Las 24 funciones
compilan por intérprete y nativo con salida idéntica; `examples/std_tests.ostrin` (6 pruebas
`--test`) y `examples/std_library.ostrin` las verifican.

Para que funcionara hubo que arreglar cuatro cosas del lenguaje/compilador:

1. **Los escalares satisfacen los bounds `Eq/Ord/Add/...`** (`builtin_type_satisfies_trait`); antes
   `fn max<T: Ord>` rechazaba `Int`, `Float` y `String`.
2. **Comparación de `String` (`< > <= >=`)** en el backend nativo (AST, HIR e IR, con `strcmp`).
3. **Especializaciones de funciones genéricas de módulos** (`lists::sorted__Int`) producían C
   inválido; se reescriben igual que los nombres de tipos calificados.
4. **Una función que termina en `return expr` (o en `if/else` con `return` en ambas ramas) daba
   E1041** («body evaluates to Void»); `block_always_returns` lo acepta y el backend AST emite el
   `if` como sentencia.

Suite: **6 diferenciales, 168 de integración y 2 unitarias**.

## 200. Asignador nativo O(1): tabla hash de asignaciones — 2026-09-20

El runtime C guardaba las asignaciones vivas en una lista enlazada y `ostrin_retain`,
`ostrin_release`, `ostrin_free` y `ostrin_realloc` la recorrían entera: con 40 000 strings vivos
el binario tardaba ~6 s (cuadrático). Ahora hay una tabla hash encadenada por puntero
(`ostrin_table_link`, crecimiento al 75 % de carga, todo bajo el mutex del heap): el mismo programa
tarda 0,07 s. `native_allocator_scales_linearly_with_live_allocations` construye 60 000 strings y
exige < 5 s y `live_allocations=0`. Suite: **6 diferenciales, 169 de integración y 2 unitarias**.

## 201. `print` de compuestos desde la IR y helpers de `show` sin fugas — 2026-09-20

`ir_c::generate` recibe ahora un gancho `show` que registra en el backend el helper
`ostrin_show_<Tipo>` y devuelve la expresión C, de modo que listas, mapas, sets, `Option`,
records y enums se imprimen desde la IR (`main` ya no cae al camino AST por un `print(lista)`).
El texto renderizado es una asignación propia: tanto la IR como el camino AST lo imprimen y lo
liberan. Además los propios helpers `ostrin_show_*` filtraban todas las cadenas intermedias
(`s = concat(s, x)` perdía el acumulador anterior y los `int_to_string`); ahora usan
`ostrin_show_cat`/`ostrin_show_catf`, que consumen el acumulador y el fragmento temporal (los
`String` de un contenedor son prestados y no se liberan). `native_ir_print_compound` imprime
lista, record, `Option` y mapa con `live_allocations=0`. Suite: **6 diferenciales, 169 de
integración y 2 unitarias**.

## 202. `ostrinc --new DIR` — 2026-09-20

Crea un proyecto listo para usar: `ostrin.toml` (nombre validado, `entry = "main.ostrin"`),
`main.ostrin` (importa `std.math`, una función y un `test_greet`) y `.gitignore`. Se niega a
tocar un directorio no vacío y a usar nombres no válidos. Flujo: `ostrinc --new hola`,
`ostrinc --project hola --run`, `ostrinc --test hola/main.ostrin`. Cubierto por
`new_scaffolds_a_project_that_runs_and_passes_its_own_test`.

## 203. Playground en el navegador (WASM) — 2026-09-20

`website/playground.{html,css,js}`: editor + salida que ejecutan el `ostrinc.wasm` real
(`wasm32-wasip1`) con `browser_wasi_shim` y un `PreopenDirectory` en memoria. Botones Run/Check/
Test/Format (`--run`, `--check`, `--test`, `--fmt`), selector de ejemplos (cuánticos, `std`,
records/enums, tests, concurrencia) y Ctrl+Enter. `pages.yml` construye el WASM en cada despliegue.
Verificado en el navegador integrado: los 5 ejemplos, un error de tipo (E1041), un error de
ejecución (división por cero) y el formateador; `--fmt` reescribe el editor. Navegación actualizada
en todas las páginas y textos de ecosistema/roadmap corregidos (el playground ya no es «futuro»).

## 204. `String.split/lines` desde la IR y transferencia de ownership — 2026-09-21

Se cerró otro tramo de la migración del backend nativo: `String.split()` y `String.lines()` ya
se emiten desde `ir_c.rs` cuando producen `List<String>`, en lugar de forzar el fallback HIR/AST.
El emisor llama al runtime C, copia los fragmentos al `List_String` gestionado y después libera
tanto cada string temporal como el array auxiliar. La misma corrección se aplicó al camino AST,
que tenía el mismo patrón de retención sin liberar el buffer de entrada.

`ownership.rs` reconoce ambos métodos como consumidores que toman prestado el receptor, por lo
que el último uso de la cadena se libera después de la llamada. `examples/native_ir_string_methods.ostrin`
ahora compara también los tamaños de `"a,b,,c".split(",")` y de una cadena con saltos CRLF.

Verificación: la prueba diferencial `native_ir_string_methods_cross_block_ownership_and_short_circuit`
pasó junto con la ejecución nativa bajo `--leak-check`; el informe publica `ir-generated: 3` y el
binario termina con `live_allocations=0`. La suite de referencia queda en **6 diferenciales, 170
de integración y 2 unitarias**.

La siguiente frontera de esta familia será ampliar `Result` a `try`, combinadores y payloads
compuestos, además de ampliar el generador diferencial con records, `Option` y closures.

## 205. `String.to_int/to_float` y `Result` escalar desde la IR — 2026-09-21

La frontera anterior ya está cerrada para la familia escalar: `String.to_int()` y
`String.to_float()` generan `Result<Int,String>`/`Result<Float,String>` directamente en el
emisor C de la IR, reutilizando las mismas reglas de parseo y los mensajes del backend HIR.
También se migraron los consumidores que hacen falta para usar esos valores sin fallback:
constructores `Ok`/`Err`, `match` sobre las variantes, bindings de `Ok(v)`/`Err(e)`,
`is_ok`, `is_err`, `unwrap_or` y `ok`.

El lowering ya conserva el discriminante de `Result` en el path del binding (`Ok.v`/`Err.e`),
en vez de reducir ambos campos a un nombre ambiguo. La ownership IR trata el wrapper como un
valor C con payloads condicionalmente gestionados: libera o retiene `value` cuando `ok` es
verdadero y `error` en caso contrario. `examples/native_ir_string_results.ostrin` cubre éxito,
error, binding, consultas y fallback, y su binario nativo coincide con el intérprete con
`live_allocations=0`.

Verificación: la prueba `native_ir_string_methods_cross_block_ownership_and_short_circuit`
pasó con **3 funciones generadas desde IR** para este ejemplo; la suite queda en **6
diferenciales, 170 de integración y 2 unitarias**. La próxima frontera concreta es ampliar
esta representación a `try`, combinadores de `Result` y payloads compuestos, manteniendo el
fallback verificable para las formas que aún no tengan contrato de ownership completo.

## 206. Propagación de `Result`/`Option` con `try` desde la IR — 2026-09-21

La IR ya consume la propagación sin `catch` que antes solo emitía el backend HIR. `try` se
representa como CFG explícito: `TryCheck` inspecciona `ok`/`has`, `TryValue` extrae el payload
en la rama normal y `TryError` construye el contenedor de error del tipo de retorno y termina
la función en la rama de propagación. Esto cubre `Result` y `Option` escalares, y no confunde
el payload de una función con el wrapper que debe retornar.

La transferencia de ownership acompaña ambos caminos: `TryValue` retiene un payload gestionado
prestado antes de liberar el wrapper fuente, mientras `TryError` retiene el error activo antes de
liberar el `Result` fuente. `try catch` inline añade `TryErrorValue`, baja el handler con su
parámetro ligado al error y construye el `Err` del tipo envolvente. `examples/native_ir_try_strings.ostrin`
verifica propagación, consulta `is_err` y recuperación con `Result<String,String>` y strings
dinámicos; el intérprete y el binario nativo imprimen `VALUE!`, `true` y `recovered: FAILURE`,
y el binario termina con `live_allocations=0`.

`native_result.ostrin` pasó de 4 funciones IR + 2 HIR a **6 funciones generadas desde IR**;
`native_result_catch.ostrin` ya genera sus 3 funciones desde IR y `try_result.ostrin` conserva
solo un fallback no relacionado con la propagación. La siguiente frontera identificada fue
migrar combinadores de `Result`, handlers no inline y payloads compuestos con el mismo contrato.

## 207. Combinadores de `Result` inline desde la IR — 2026-09-21

`Result.map`, `Result.map_err` y `Result.then` con lambdas inline ya se expanden en CFG nativo.
Cada operación inspecciona el discriminante con `TryCheck`, extrae el payload activo mediante
`TryValue`/`TryErrorValue`, ejecuta el cuerpo de la lambda en la rama correspondiente y combina
los dos resultados con `Phi`; las ramas que no transforman el payload reconstruyen `Ok`/`Err`
con el tipo de retorno concreto.

El typechecker ahora propaga el tipo contextual parcial dentro de constructores como `Ok(x)` y
`Err(e)`, de modo que un `then` que devuelve `Result<U,E>` conserva el `E` del receptor aunque
`U` se infiera dentro de la lambda. `examples/native_ir_result_combinators.ostrin` cubre
transformación de éxito, propagación de error, `map_err` en ambas variantes y `then`; genera sus
9 funciones desde IR, coincide con el intérprete y termina con `live_allocations=0`.

La ownership IR queda sin valores ni funciones irresueltos en ese ejemplo. Siguen pendientes los
combinadores de `Option`, handlers no inline, payloads compuestos y la expansión completa de
closures que escapan del sitio de llamada.

## 208. Combinadores de `Option` inline desde la IR — 2026-09-21

`Option.map` y `Option.then` con lambdas inline ya comparten el mismo lowering CFG que `Result`.
La rama `Some` extrae el payload con `TryValue`, ejecuta la lambda y construye el nuevo `Some`
cuando corresponde; la rama `None` usa `TryError` para conservar el vacío con el tipo de salida.
Ambas ramas se reúnen con `Phi`, y los payloads `String` siguen las retenciones/liberaciones
del runtime nativo.

`examples/native_ir_option_combinators.ostrin` cubre `Some` y `None` para `map` y `then`, tanto
con `Option<String>` como con `Option<Int>`. Sus 9 funciones se generan desde IR, el intérprete
y el binario nativo imprimen `mapped: VALUE`, `none`, `5`, `none`, y `--leak-check` termina con
`live_allocations=0`. Quedan para una siguiente etapa los handlers no inline, payloads compuestos
que no tengan lowering completo y closures que escapen del sitio de llamada.

## 209. Handlers globales de `try catch` desde la IR — 2026-09-21

Un `try catch` cuyo handler es una función global compatible (`catch recover`) ya se baja sin
crear una función-valor opaca: la rama de error extrae el `String` con `TryErrorValue`, emite
`call recover(error)` y construye el `Err` del resultado envolvente. El handler recibe el
parámetro como argumento prestado y el resultado nuevo conserva el contrato normal de
`Result`/ownership.

`examples/native_ir_try_handler.ostrin` cubre el camino de éxito y el de recuperación de error;
las 6 funciones generan IR, el intérprete y el binario nativo imprimen `VALUE!` y
`handled: FAILURE`, y `--leak-check` termina con `live_allocations=0`. Siguen pendientes los
handlers locales/closures no inline y los payloads compuestos que aún no tengan representación
completa en el emisor IR.

## 210. Payloads compuestos gestionados en `Option`/`Result` — 2026-09-21

La IR nativa ya representa una capa de agregados gestionados dentro de los wrappers: listas
escalares o de records pueden viajar como `Option<List<T>>` y como payload activo de
`Result<List<T>, E>`. El mangle recursivo produce los mismos nombres ABI que el generador C
genérico (`Option_List_String`, `Result_List_String_String`), y `Some`/`Ok`/`Err`, `match`,
`TryValue`/`TryErrorValue`, `Phi` y los marcadores de ownership retienen o liberan la lista
en el punto correcto.

`examples/native_ir_option_list.ostrin` cubre `Some`/`None`, `Ok`/`Err`, indexación y métodos
de `List<String>`. Sus 5 funciones se generan desde IR, coinciden con el intérprete y el
binario nativo termina con `live_allocations=0`. Mapas/sets anidados y wrappers compuestos
recursivos siguen fuera de este bloque hasta fijar su contrato de ownership específico.

## 211. Mapas y conjuntos dentro de wrappers nativos — 2026-09-21

La misma capa de ownership ya cubre `Option<Map<Int,String>>` y `Result<Set<Int>,String>`.
El emisor IR reconoce sus mangles (`Option_Map_Int_String` y `Result_Set_Int_String`),
conserva el puntero del contenedor activo en `Some`/`Ok`/`Err` y delega la destrucción de
entradas y valores a los callbacks de `Map`/`Set`. El patrón `match` y las consultas
`count` se mantienen en IR sin degradar a HIR.

`examples/native_ir_compound_collections.ostrin` genera sus 5 funciones desde IR, coincide
con el intérprete y termina con `live_allocations=0` (7 allocations totales en el escenario
de prueba). Quedan para otro bloque los handlers locales o closures no inline y consumidores
complejos de wrappers anidados.

## 212. Ownership recursivo para wrappers anidados — 2026-09-21

`Option` y `Result` ya pueden contener otros wrappers por valor en la IR nativa, por ejemplo
`Option<Option<String>>` y `Result<Option<String>,String>`. El emisor calcula los typedefs en
orden topológico cuando un `Result` depende de un `Option`, y los helpers de ownership recorren
el discriminante interno antes de llamar a `ostrin_retain`/`ostrin_release`; así una copia del
wrapper conserva únicamente el payload gestionado que está activo.

`examples/native_ir_nested_wrappers.ostrin` verifica constructores anidados, `match` anidado,
`Some`/`None`, `Ok`/`Err`, strings dinámicos y cleanup nativo. Sus 5 funciones generan IR,
coinciden con el intérprete y terminan con `live_allocations=0`. Consumidores como `unwrap` de
un wrapper gestionado anidado y handlers locales/closures no inline siguen siendo el siguiente
frente porque requieren transferencias adicionales o llamadas indirectas.

## 213. Aliases locales de handlers globales en `try catch` — 2026-09-21

El lowering de la IR conserva ahora el origen estático de un valor de función global cuando se
asigna a un binding local. Así, `handler = recover; try fail() catch handler` reconoce que el
alias apunta a `recover` y emite la misma llamada estática que el handler escrito directamente;
no crea una función-valor opaca ni degrada la función completa a HIR/AST.

Para este primer tramo, los valores `Fn` se representan en el C de la IR como punteros estáticos
`void*` solo para conservar la procedencia y el bookkeeping del alias; las operaciones de
ownership sobre ellos son no-op. Esto no pretende habilitar todavía llamadas indirectas, aliasing
a través de `Phi` ni closures con entorno capturado.

`examples/native_ir_try_local_handler.ostrin` cubre aliases en las ramas de éxito y error. Sus 6
funciones generan IR, el intérprete y el binario nativo imprimen `VALUE!` y `handled: FAILURE`, y
`--leak-check` termina con `live_allocations=0`. La batería queda en **6 pruebas diferenciales,
176 de integración y 2 unitarias**. El siguiente frente de esta línea son llamadas indirectas y
handlers locales que sean closures con entorno.

## 214. Base de descubrimiento y playground vivo en la web — 2026-09-21

La primera fase del plan de website 2.0 queda aterrizada sin sustituir la infraestructura actual:

- `docs/website-audit.md` registra la auditoría del sitio, sus cifras comprobadas y el orden de
  entrega. La fuente sigue siendo el repositorio: 192 programas `.ostrin`, 22 documentos de
  diseño y la suite actual de 176 integraciones, 6 diferenciales y 2 unitarias.
- La portada reutiliza `website/playground.js` y el `ostrinc.wasm` real que construye `pages.yml`;
  ahora ofrece Run, Check, Test, Format y Share desde una sección compacta. No hay resultados
  simulados: el programa se ejecuta en el navegador a través del host WASI en memoria.
- El playground acepta `?code=...` y puede copiar una URL compartible. La compilación del módulo
  WASM sigue siendo compartida dentro de cada página y el backend C no se anuncia como disponible
  en navegador.
- Todas las páginas públicas tienen títulos/descripciones más claros y canonical/OG básicos;
  `robots.txt`, `sitemap.xml` y JSON-LD de `WebSite`/`SoftwareSourceCode` preparan el descubrimiento
  sin afirmar una distribución instalable o un registro de paquetes que todavía no existe.
- Se corrigieron contadores y textos que aún llamaban futuro al playground. Se añadió una sección
  explícita “Beyond science” para records, colecciones, `Result` y concurrencia reales.

Se preservaron las páginas estáticas, el estilo visual, el logo, el flujo GitHub Pages y el
compilador WASM existente. Quedan para bloques posteriores el showcase, la comunidad y el
fortalecimiento de la experiencia documental.

## 215. Catálogo live y validación del sitio — 2026-09-21

La siguiente fase convierte el catálogo en una superficie ejecutable y deja sus invariantes en CI:

- `website/playground.js` ahora tiene una ruta común para el playground completo y componentes
  `data-live-example`, compartiendo la promesa de ejecución y el módulo WebAssembly compilado.
- `website/examples.html` incorpora cuatro demos editables y reales: cantidades, biblioteca estándar,
  records/enums y concurrencia. Cada una permite Run, Check, Reset y Copy, además de enlaces a fuente
  y documentación relacionada.
- `scripts/website-check.mjs` valida páginas públicas, títulos/canonical/OG/Twitter, referencias
  locales y anclas, wiring del playground, sitemap/robots y que las cuatro fuentes live no se
  separen de sus ejemplos validados. `ci.yml` lo ejecuta en cada cambio y `pages.yml` lo repite
  después de generar `ostrinc.wasm`, incluyendo un umbral básico de tamaño del artefacto.

No se añadió un framework ni un backend de snippets: las demos siguen siendo estáticas en su entrega,
pero la ejecución ocurre dentro del navegador con el compilador real. El siguiente paso de producto es
un showcase honesto y la preparación de comunidad; el siguiente paso técnico es mejorar diagnósticos
y conectar los metadatos de demos con los ejemplos validados por la suite.

## 216. Showcase source-backed y preparación de comunidad — 2026-09-21

La Fase D inicial queda implementada sobre evidencia existente, sin simular una comunidad madura:

- `website/showcase.html` presenta cuatro programas reales del repositorio: cantidades físicas,
  tablas/CSV, SVG determinista y autodiff. Cada tarjeta enlaza al código y al test que protege el
  comportamiento; el paquete de plotting se marca como `early` y no se anuncia como gráfico web
  interactivo.
- `website/community.html` ofrece rutas concretas hacia GitHub, issues, contributing, diseño,
  roadmap y code of conduct. Declara explícitamente que Issues/PRs son las superficies activas y
  que chat, Discussions, registry y proyectos externos no se anuncian hasta existir.
- `CONTRIBUTING.md` añade `Good first contribution`; `.github/ISSUE_TEMPLATE/` añade bug, feature,
  language proposal, package proposal y documentation; `.github/pull_request_template.md` conecta
  cambios con evidencia y checks; `docs/community-labels.md` deja la taxonomía como propuesta para
  mantenedores, sin crear labels remotamente.
- La navegación pública y el sitemap incluyen Showcase y Community; `site.js` agrega esos enlaces
  a las páginas antiguas que conservan su markup estático.

Se preservaron el logo, los estilos, el playground, la infraestructura de Pages y las superficies
existentes. Quedan para el siguiente bloque el tutorial guiado y una mejora de diagnósticos/learning
funnel, además de ampliar la experiencia científica solo cuando las APIs reales lo soporten.

## 217. Ruta guiada de aprendizaje — 2026-09-21

La documentación pública deja de ser únicamente un índice de decisiones y ahora ofrece una primera
ruta de aprendizaje enlazada con evidencia del repositorio:

- `website/docs.html` añade catorce pasos, desde ejecutar Ostrin y sus fundamentos hasta colecciones,
  records/enums, `Option`/`Result`, patrones, cantidades, estadística, concurrencia, paquetes,
  compilación nativa y un proyecto completo. Cada paso apunta a un ejemplo real, una sección de
  referencia, el playground o el showcase; no se inventan capítulos para capacidades ausentes.
- Las etiquetas `available` y `early` distinguen lo que ya tiene una superficie ejecutable y lo que
  existe pero aún necesita una historia de tutorial o distribución más madura. El texto explica cómo
  usar cada paso y conserva la advertencia de que el estado describe implementación actual.
- El CSS mantiene la composición visual del sitio, añade una cuadrícula responsive para la ruta y
  conserva el comportamiento de una columna en pantallas pequeñas. El showcase expone anclas
  estables para las tarjetas de cantidades y tablas.
- `scripts/website-check.mjs` comprueba la sección guiada, las fuentes clave y los enlaces de
  referencia. El check completo queda en **9 páginas públicas**, y la suite del compilador sigue en
  **6 diferenciales, 176 integraciones y 2 unitarias** en verde.

La mejora se limita a documentación respaldada por código existente; los diagnósticos enriquecidos y
la distribución instalable quedan como el siguiente incremento del learning funnel.

## 218. Diagnósticos estructurados en el playground — 2026-09-21

El siguiente incremento del learning funnel mejora la primera experiencia de error sin modificar el
checker ni ocultar su salida real:

- `website/playground.js` solicita `--json` junto con `--check` y `--run`. El parser acepta JSON Lines
  emitidos por stdout o stderr porque el binario WASI puede dirigirlos por cualquiera de los dos
  canales según la ruta de diagnóstico.
- Los diagnósticos estructurados se muestran como filas legibles con código `OSTRIN-Exxxx`, archivo,
  línea, columna, severidad y mensaje. Traps y salida no estructurada conservan el fallback textual.
  El encabezado resume la cantidad de errores/advertencias y el tiempo de ejecución.
- La salida completa del playground y la portada usa ahora un contenedor `role="status"`; los estilos
  permiten wrapping en pantallas estrechas. Los live examples comparten el renderizado de diagnósticos
  sin perder sus outputs normales.
- `scripts/website-check.mjs` protege el wiring de `--json`, el parser y la semántica accesible del
  output. En el navegador, un programa inválido produjo el diagnóstico real
  `OSTRIN-E1024 main.ostrin:4:11 Invalid dimensional operation. Cannot add/subtract Length and Time.`;
  el programa válido siguió devolviendo `5 m/s`.

No se añadió una capa de mensajes inventados ni se reemplazó el compilador WASM. Quedan para próximos
bloques el resaltado de línea en el editor y una prueba responsive móvil con un viewport dedicado.

## 219. Selección de línea diagnosticada — 2026-09-21

La experiencia de diagnóstico da un paso más sin incorporar CodeMirror, Monaco ni dependencias nuevas:

- `website/playground.js` calcula el rango de la primera ubicación JSON válida, enfoca el `textarea` y
  selecciona toda la línea diagnosticada. La cabecera del editor muestra `line N · column M`, y un
  diagnóstico ausente o una ejecución válida limpia el estado visual.
- `website/index.html` y `website/playground.html` comparten el nuevo indicador accesible de ubicación;
  `playground.css` marca el editor con una línea lateral y conserva el wrapping del output. Los live
  examples también seleccionan la primera línea cuando su `Check` devuelve un error.
- `scripts/website-check.mjs` protege el indicador de ubicación y el contrato accesible del output.
  En el navegador, el error dimensional real seleccionó `print(distance + time)` en la línea 4,
  mostró `line 4 · column 11` y mantuvo el mensaje `OSTRIN-E1024`; al recargar, el ejemplo válido
  limpió el marcador y devolvió `5 m/s`.

La mejora sigue siendo nativa del navegador y no altera la semántica del compilador. La validación de
un viewport móvil dedicado queda pendiente porque la superficie CUA disponible no expone un override
de viewport; la regla responsive de una columna permanece en CSS.

## 220. Contrato verificable de los artefactos nativos de release — 2026-09-21

El workflow `.github/workflows/release.yml` ya no se limita a compilar, comprimir y calcular un
checksum. Antes de subir cada artefacto comprueba el contrato completo del paquete:

- obtiene la versión desde `compiler/Cargo.toml` mediante `cargo metadata` y rechaza una etiqueta
  `v<version>` que no coincida;
- ejecuta el binario construido con `--version`, `--check examples/hello.ostrin` y `--run`,
  verificando la salida `hola desde Ostrin`;
- valida el `*.sha256`, extrae el archivo a un directorio temporal y vuelve a ejecutar el binario
  extraído con `--version` y `examples/hello.ostrin`;
- ejecuta desde el paquete extraído `examples/pkg_project/main_app` con `--locked --run`, comprobando la
  dependencia local `path` y la salida `hola, Ostrin`;
- normaliza el nombre derivado de la rama en ejecuciones manuales para que una rama con `/` no rompa
  la creación del archivo.

La matriz continúa cubriendo Linux x86_64, macOS arm64 y Windows x64. Esto hace que el workflow sea
una verificación reproducible de los archivos entregados, aunque todavía no publica una release por
sí mismo sin una etiqueta válida y tampoco sustituye el trabajo pendiente de un instalador.

Verificación local de esta modificación: `git diff --check`, `cargo test --manifest-path
compiler/Cargo.toml` (2 unitarias, 6 diferenciales y 176 integraciones en verde), además de los
checks existentes del sitio y del WASM. El cambio queda preparado para que la primera etiqueta de
release falle de forma explícita ante una inconsistencia de versión, checksum o contenido ejecutable.

## 221. Biblioteca estándar de fechas deterministas — 2026-09-21

La biblioteca estándar embebida gana su primer módulo de calendario con `compiler/std/time.ostrin`:

- `Date` es un `record` público con `year`, `month` y `day`, construido mediante `time.date`;
- `is_leap_year`, `days_in_year`, `days_in_month` e `is_valid` implementan el calendario gregoriano
  proléptico y rechazan años no positivos o días imposibles;
- `day_of_year`, `from_day_of_year` y `day_of_week` cubren ordinales, conversión inversa y día ISO
  (lunes = 1, domingo = 7);
- `iso` formatea `YYYY-MM-DD` y `parse_iso` devuelve `Result<Date, String>` con errores explícitos,
  sin consultar el reloj ni la zona horaria del sistema.

El módulo se registra en el cargador virtual `std`, por lo que el mismo código Ostrin se ejecuta en el
intérprete y en el backend nativo. `examples/std_tests.ostrin` añade la validación de años bisiestos,
ordinales, parseo y fechas inválidas; `examples/time_library.ostrin` comprueba la salida pública y la
paridad de ambos backends. La prueba nativa con `--leak-check` termina con `live_allocations=0`,
incluidas las temporales de `iso`; la suite pasa ahora **7 pruebas propias de std**, además de los
checks existentes. JSON y red permanecen deliberadamente fuera de este bloque.

## 222. Integridad de dependencias en lockfiles — 2026-09-21

El sistema de paquetes deja de confiar únicamente en la ruta, versión y commit resueltos:

- `compiler/src/package.rs` calcula un SHA-256 determinista sobre `ostrin.toml` y todos los archivos
  `.ostrin`, con finales de línea normalizados a LF, ordenados por ruta relativa y excluyendo
  `.git`/`.ostrin` de caché;
- cada entrada de `ostrin.lock` conserva `content_sha256` junto a `source`, `resolved_path`, versión
  y, para Git, `resolved_rev`;
- las dependencias `path` y Git verifican el hash en builds normales y con `--locked`, sin red nueva;
  si falta o cambia el contenido, el compilador exige regenerar el lockfile;
- `compiler/tests/examples.rs` comprueba el campo SHA-256 y rechaza una modificación posterior de
  una dependencia local. El fixture `examples/pkg_project/main_app/ostrin.lock` ya usa el nuevo campo.

La medida resuelve la pregunta abierta de integridad del diseño de paquetes sin crear todavía un
registro remoto ni cambiar la resolución explícita existente.

## 223. Temporales gestionados en el fallback nativo — 2026-09-21

La ruta AST que todavía cubre consumidores no representables por la IR ya aplica ownership explícito
en el borde de las llamadas:

- los argumentos frescos gestionados de llamadas ordinarias y genéricas se materializan en temporales C;
  el callee los toma prestados y el caller libera la referencia al volver, sin tocar locales nombrados;
- `print` libera el valor gestionado temporal después de construir y liberar su representación textual;
- un acceso a campo desde un record temporal conserva el campo si es gestionado y libera el record
  después de leerlo, cubriendo expresiones como `parse_iso(...).unwrap().day`;
- expresiones gestionadas descartadas en statements, tails de bloques y cuerpos `Void` también se
  liberan cuando su origen es una construcción fresca.

La regresión `std_library_modules_agree_between_backends_and_pass_their_own_tests` ejecuta ahora el
ejemplo completo con `--leak-check` y exige `live_allocations=0`; el caso incluye listas genéricas,
strings, records de fechas y resultados. `native_ir_lists.ostrin` sigue verificando que reutilizar
un local nombrado no provoque una liberación prematura.

La IR continúa siendo el destino de ownership de largo plazo; esta capa evita que la frontera AST
introduzca fugas mientras permanecen pendientes los consumidores complejos, escapes y llamadas
indirectas.

## 224. Rangos enteros en la IR nativa — 2026-09-21

La migración del backend nativo cierra ahora la familia de `for` sobre rangos enteros:

- `compiler/src/ir.rs` baja `start to/until end` directamente a un CFG con `phi` para el índice,
  ramas separadas para pasos positivos y negativos, y una salida común para el paso cero;
- un `step` se evalúa una sola vez, igual que en el intérprete. La ruta conserva límites inclusivos
  (`to`) y exclusivos (`until`) y reutiliza los edges de `break`/`continue` y las `phi` de bindings
  mutados del cuerpo;
- `examples/native_ir_ranges.ostrin` cubre ascenso, descenso, paso cero, `continue` y `break`.
  El intérprete y el binario nativo producen `25`, `20`, `19`, `0`; el reporte marca sus cinco
  funciones como `ir-generated` y ninguna como HIR, y `--leak-check` informa cero asignaciones vivas.

Esto retira un `Opaque` concreto de la ruta de rangos sin alterar los rangos con cantidades ni los
iteradores propios genéricos/indirectos, que siguen en las capas de fallback verificadas.

## 225. Iteradores de records concretos en la IR nativa — 2026-09-21

La migración del backend nativo cubre ahora el protocolo de iteradores definido por el usuario para
records concretos:

- `HirProgram` conserva la asociación `record -> T` de cada `impl Iterator<T>` y la bajada IR usa esa
  metadata para construir el `for` como un CFG con condición, extracción `TryValue` y edges de
  `break`/`continue`, sin volver a inferir el tipo desde el AST;
- `ir_c.rs` resuelve `next()` contra la tabla de métodos C registrada por `codegen`, de modo que el
  consumidor emite una llamada estática como `Fibonacci__next` y mantiene `Option<T>` por valor;
- `examples/fibonacci.ostrin` compara intérprete y nativo, exige que el consumidor sea `ir-generated`,
  inspecciona la llamada C emitida y ejecuta `--leak-check` con `live_allocations=0`.

Los iteradores genéricos, indirectos o con payloads que el emisor aún no soporta conservan el fallback
HIR/AST. La frontera es deliberada: añade una familia real de consumidores a la IR sin afirmar que el
protocolo de iteración completo ya esté migrado.

## 226. Iteración de canales en la IR nativa — 2026-09-21

El backend nativo ya no necesita abandonar la IR para el caso síncrono de canales sin tareas:

- `for value in channel` se baja al mismo CFG de polling `Option<T>` que los iteradores de records,
  usando `ChannelReceive`, `TryCheck`, `TryValue` y edges explícitos de `continue`/salida;
- `ChannelNew`, `ChannelSend`, `ChannelReceive` y `ChannelClose` tienen ahora una representación C
  conservadora para los helpers ya existentes del runtime (`Channel_<T>_*`), incluidos canales de
  strings y records cuando el payload tiene representación soportada;
- la pasada de ownership trata el handle como referencia gestionada y acepta `send`, `receive` y
  `close` como usos prestados, colocando `release` después del último receive. El ejemplo
  `native_ir_channel_iterator.ostrin` compara ambos backends, exige `ir-generated` y termina con
  `live_allocations=0`.

`spawn`/`Task` y la coordinación entre tareas siguen en HIR/AST; este bloque migra el consumidor de
`Channel<T>` y su contrato de memoria sin confundirlo con la migración completa de concurrencia.

## 227. Spawn y Task.join en la IR nativa — 2026-09-21

La frontera HIR→IR→C incorpora ahora una primera forma ejecutable de concurrencia real:

- `spawn {}` sin capturas, `spawn_scope` y control de flujo interno conservan el fallback verificado;
  una región lineal sin valores prestados del llamador se convierte en un callback C estático con
  su propio conjunto de temporales, de modo que el backend no inventa una ABI de entorno parcial;
- `Task<T>` tiene representación nativa gestionada, inicialización mediante el mismo runtime que
  usa el emisor HIR (`ostrin_register_task`, `ostrin_track_task_handle` y, con
  `OSTRIN_NATIVE_THREADS`, `Task_<T>_start`), y `Task.join()` llama a `Task_<T>_join`;
- el análisis de ownership trata el handle como una referencia gestionada y puede liberar el
  último uso después del `join`. La regresión `examples/native_ir_spawn_join.ostrin` compara
  intérprete y nativo, exige `ir-generated`, inspecciona el callback y el helper C, y ejecuta
  tanto el scheduler cooperativo como los hilos nativos con `live_allocations=0`.

La migración no declara resuelta toda la concurrencia: faltan la ABI de entornos para capturas,
la bajada de scopes y las regiones con ramas o loops. Esas formas permanecen observables como
fallback HIR/AST hasta que ownership, cancelación y escape puedan expresarse en la IR sin perder
la semántica del intérprete.

## 228. Capturas inmutables en tareas IR — 2026-09-21

La siguiente extensión de la concurrencia nativa conserva la semántica de captura por valor sin
hacer que el callback dependa de temporales del llamador:

- una región lineal puede leer valores inmutables definidos en la función padre; el emisor detecta
  esos usos, genera un `OstrinIrTaskEnv_*` con campos C tipados y sustituye las lecturas por campos
  del entorno dentro del callback;
- la creación de la tarea reserva el entorno con el allocator registrado, copia los campos y
  retiene los payloads gestionados. El destructor del entorno libera cada campo y después el propio
  entorno, también si una tarea se cancela antes de ejecutarse;
- cuando el resultado es una referencia capturada directamente, el callback retiene el valor antes
  de que el runtime destruya el entorno. La regresión `native_ir_spawn_join.ostrin` cubre capturas
  escalares, una captura `String`, una tarea sin capturas, `Task_Int_join` y `Task_String_join` en
  scheduler cooperativo y en hilos nativos, con `live_allocations=0`.

La frontera sigue siendo intencionalmente estrecha: las capturas mutables ya son rechazadas por el
checker, y las tareas anidadas o `spawn_scope` permanecen en HIR/AST hasta que la IR pueda
representar su cancelación y ownership de escape de forma equivalente.

## 229. CFG interno de tareas en la IR nativa — 2026-09-21

Las regiones de `spawn` ya no están limitadas a un único bloque lineal:

- el emisor descubre el subgrafo alcanzable desde la región de tarea, conserva sus `Goto`,
  `Branch`, `Phi`, backedges y múltiples `RegionReturn`, y lo emite como un callback C con labels
  y un predictor local, sin mezclarlo con el CFG de la función padre;
- las capturas se calculan sobre todos los bloques de la región, no sólo sobre el bloque raíz, y
  el ownership modela cada lectura externa como transferencia en `Spawn`. Los valores gestionados
  producidos dentro de la región se transfieren por `RegionReturn` y pueden liberar sus temporales
  internos en los edges seguros;
- `examples/native_ir_spawn_join.ostrin` añade una tarea con una condición capturada y dos
  ramas, exige que el C contenga el entorno y los labels de la región, y mantiene la comparación
  con el intérprete, los hilos nativos y `live_allocations=0`.

La bajada sigue rechazando tareas anidadas, `spawn_scope`, retornos normales que escapen de la
región y formas cuya ownership no pueda probarse; esas formas permanecen en el fallback verificado.

## 230. `spawn_scope` inline en la IR nativa — 2026-09-21

La concurrencia estructurada cruza ahora la misma frontera HIR→IR→C para el caso que no requiere
tareas anidadas:

- `SpawnScope` deja de representarse como una tarea artificial. La IR abre un grupo con
  `scope_begin`, baja el cuerpo en el CFG de la función y lo cierra con `scope_end`, por lo que
  los `spawn` hijos quedan asociados al grupo real del runtime y se drenan antes de continuar;
- los retornos explícitos dentro del ámbito cierran primero los grupos activos en la ruta de salida;
  `break` y `continue` llevan la profundidad del loop para cerrar sólo los scopes que realmente
  atraviesan. El emisor C declara los marcos de grupo por función y conserva el comportamiento
  cooperativo y de `--native-threads` del runtime existente, incluida la cancelación propagada y
  el drenado;
- `examples/native_ir_spawn_join.ostrin` añade un `spawn_scope` con hijo nativo, compara intérprete,
  C cooperativo e hilos nativos, exige `ir-generated: 1`, y mantiene `ownership-ir unresolved-values: 0`
  y `live_allocations=0`.

La frontera sigue siendo deliberada: tareas anidadas dentro de callbacks nativos, scopes que
escapan por formas de control no modeladas y la cancelación que requiere saltar una ABI de callback
siguen en HIR/AST hasta que puedan expresar su cleanup estructurado sin perder ownership.

## 231. Tareas anidadas y propagación de entornos en la IR — 2026-09-21

Los callbacks nativos pueden ahora crear otra tarea con la misma ABI cuando la composición es
estática y verificable:

- los helpers de `spawn` se preparan con nombres estables antes de emitir cuerpos, de modo que un
  callback padre puede registrar un hijo sin depender del orden de generación de los helpers;
- las capturas del hijo se propagan hacia el entorno del padre cuando cruzan ese límite. Cada entorno
  conserva su propio retain/release, destructor y resultado gestionado; el hijo recibe una copia
  retenida y `join()` conserva el contrato del runtime cooperativo y de hilos nativos;
- `examples/native_ir_spawn_join.ostrin` cubre ahora un `spawn_scope`, una tarea anidada y una
  captura `String` que atraviesa ambos entornos, con paridad de salida y `live_allocations=0`.

Scopes anidados dentro de callbacks, control que escape del ámbito y formas indirectas todavía
conservan el fallback HIR/AST; la propagación sólo se acepta cuando ownership puede expresarse en
la cadena completa de callbacks.

## 232. `Task.cancel()` tipado en la IR nativa — 2026-09-21

La operación explícita de cancelación ya cruza la frontera IR→C para tareas cuyo payload tiene
representación nativa:

- `HirKind::MethodCall` conserva `Task.cancel()` como una operación de método y el emisor IR-C
  selecciona `Task_<T>_cancel`, validando payload, aridad y resultado `Bool` antes de generar C;
- el análisis de ownership reconoce la operación como llamada prestataria: el handle puede
  liberarse después de su último `cancel()` sin convertir la solicitud en un movimiento;
- `examples/native_ir_task_cancel.ostrin` exige `ir-generated: 1`, compara el scheduler
  cooperativo con el intérprete, compila y ejecuta con `--native-threads`, y verifica
  `live_allocations=0`.

Esto no declara preempción: la semántica sigue siendo la del runtime existente —cancelación
inmediata para tareas pendientes y solicitud cooperativa para tareas en ejecución—, mientras
las formas indirectas o los escapes complejos conservan el fallback verificado.

## 233. `yield()` en la IR nativa — 2026-09-21

La cesión explícita del scheduler ya no fuerza el fallback HIR/AST en funciones IR compatibles:

- el emisor reconoce la llamada builtin sin convertirla en una función Ostrin ficticia y genera
  una expresión C que selecciona `ostrin_poll_one()` en el scheduler cooperativo o
  `ostrin_select_wait()` bajo `OSTRIN_NATIVE_THREADS`;
- ambos caminos ejecutan después `ostrin_task_checkpoint()`, de modo que la cancelación conserva
  el mismo punto seguro que en el emisor HIR;
- `examples/native_ir_yield.ostrin` combina `spawn`, `yield()` y `Task.join()`, exige
  `ir-generated: 1`/`hir-generated: 0`, y verifica los dos modos con `live_allocations=0`.

La selección entre canales quedó cubierta después en la sección 235, donde la lista tipada y su
ownership temporal se bajan sin duplicar la lógica de `select` del runtime.

## 234. Ownership de canales en tareas IR — 2026-09-21

La activación de `yield()` sobre programas de concurrencia existentes descubrió una referencia
que debía expresarse antes de ampliar la cobertura: los entornos C de tareas retienen y liberan
`Channel<T>` igual que `Task<T>`, permitiendo que varios callbacks compartan un canal sin depender
accidentalmente del binding del padre.

La regresión de recepción bloqueada ahora cruza la IR, cancela el consumidor, drena el scope y
termina con `live_allocations=0`; la suite completa conserva la misma paridad con el intérprete.

## 235. `select(List<Channel<T>>)` en la IR nativa — 2026-09-21

La coordinación entre canales ya tiene una primera bajada nativa verificable:

- `List<Channel<T>>` recibe una representación IR-C (`List_Channel_<T>`) y conserva el ownership
  de cada handle mediante el destructor generado de listas;
- la llamada `select` valida el payload y el resultado `Option<T>`, recorre los canales en el
  mismo orden determinista, usa `Channel_<T>_try_receive` y espera con `ostrin_poll_all()` o
  `ostrin_select_wait()` según el modo, seguido del checkpoint de cancelación;
- `examples/native_ir_select.ostrin` exige IR puro, compara intérprete y nativo y verifica
  scheduler cooperativo, hilos nativos y `live_allocations=0`.

La cobertura inicial usa un canal ya listo; selección bloqueante, cancelación durante la espera
y listas indirectas quedan como la siguiente ampliación verificable.

## 236. Grafo transitivo de paquetes y lockfiles — 2026-09-21

La resolución de paquetes dejó de limitarse a las dependencias directas del proyecto:

- cada dependencia resuelta que contiene `ostrin.toml` se inspecciona recursivamente y sus
  dependencias quedan disponibles para imports mediante el alias declarado por ese paquete;
- el espacio de nombres plano del cargador se protege rechazando alias duplicados y ciclos de
  directorios, en vez de escoger silenciosamente una ruta; dependencias Git transitivas conservan
  la política existente de red explícita (`--fetch`);
- `ostrin.lock` registra todos los nodos del grafo con versión, fuente, commit cuando aplica y
  `content_sha256`. Las rutas se calculan relativas al proyecto raíz incluso cuando el paquete
  transitivo está fuera de su directorio inmediato.

`transitive_path_dependencies_resolve_and_lock_reproducibly` crea un proyecto temporal con dos
niveles de dependencias, comprueba el import transitivo, la entrada de ambos nodos en el lockfile,
la ejecución `--locked` y que el lockfile no se reescriba. La ruta Git, el registro remoto y los
workspaces siguen fuera de este bloque; no se ha añadido red implícita ni un registro central.

## 237. Instalación verificable de releases nativos — 2026-09-21

La distribución nativa ya tenía un contrato de archivos por plataforma, pero todavía no ofrecía
una ruta reproducible de instalación para una persona que no quisiera compilar Rust:

- `scripts/install.sh` resuelve la release etiquetada, selecciona Linux x86_64 o macOS arm64,
  descarga el archive y su `.sha256`, verifica el digest antes de extraerlo y comprueba la salida
  de `ostrinc --version` después de una instalación atómica en `~/.local/bin` (o el directorio
  elegido por `--install-dir`);
- `scripts/install.ps1` hace el mismo recorrido para Windows x64 mediante `Get-FileHash`,
  `Expand-Archive` y una opción explícita `-AddToPath`; ambos instaladores fallan si no existe
  una release publicada y no convierten `main` en una fuente de binarios confiable;
- `scripts/distribution-check.mjs` y el job de CI verifican que la matriz de targets, el naming de
  archives, los checksums, los instaladores y la documentación sigan describiendo el mismo
  contrato. La release workflow continúa siendo la autoridad que construye y smoke-testea los
  binarios.

La publicación de una primera etiqueta `v<version>` sigue siendo una decisión de mantenimiento
externa al código: hasta entonces la web y los instaladores declaran honestamente que no existe
una descarga pública. El navegador, el workflow WASI, las páginas existentes y la CLI `ostrinc`
se conservaron sin introducir un alias o una herramienta nueva incompatible.

## 238. Utilidades de texto en `std.strings` — 2026-09-21

La biblioteca estándar amplía su superficie de texto sin añadir una segunda semántica de
`String`: las funciones libres delegan en los métodos y builtins ya soportados por ambos backends:

- `trim` elimina espacios periféricos, `split` separa por un delimitador y `lines` devuelve las
  líneas como `List<String>`;
- `is_blank` expresa el predicado habitual sin repetir `trim().is_empty()` en cada programa;
- `format_text` expone el formateador de placeholders existente con un nombre de módulo estable,
  conservando el contrato `String + List<String> -> String`.

`examples/std_tests.ostrin` cubre las funciones y `examples/std_library.ostrin` las muestra en la
comparación intérprete↔nativo. El caso nativo conserva `live_allocations=0`, y el playground
reutiliza el mismo programa source-backed; la red sigue explícitamente fuera de esa entrega.

## 239. `std.json` y cierre de ownership en texto — 2026-09-21

La biblioteca estándar incorpora ahora un módulo JSON real, escrito en Ostrin y compartido por el
intérprete y el backend nativo:

- `std.json` define `Kind` y `Value` como un DOM con valores nulos, booleanos, números, texto,
  arrays y objetos. Expone constructores, accesores, `object_get`/`object_keys`, `parse` y
  `stringify`, sin introducir una representación privada distinta para cada backend;
- el parser valida la gramática de números, literales, separadores, claves duplicadas, escapes y
  pares sustitutos UTF-16. Convierte los pares a UTF-8 y rechaza `\\u0000`, porque el `String`
  actual no representa NUL embebido. La serialización es determinista y conserva el orden de las
  claves recibidas;
- la primera prueba nativa reveló y permitió cerrar tres transferencias: un `Value` recién
  parseado debe ceder su referencia al `List<Value>`, las concatenaciones intermedias no deben
  dejar buffers vivos y `String.codepoint()` debe liberar sólo receptores frescos, no bindings
  prestados. El arreglo queda cubierto por `--leak-check` y por la prueba de `std.strings`;
- `examples/std_tests.ostrin` pasa ocho pruebas, `examples/std_library.ostrin` incluye JSON y
  `json_library.ostrin` compara cuatro líneas exactas entre intérprete y nativo. Ambos ejecutables
  nativos terminan con `live_allocations=0`; `website/playground.js` reutiliza la fuente validada.

La red y un registro remoto de paquetes siguen fuera de este bloque. El siguiente avance debe
conservar la misma regla: cada superficie nueva entra con programa ejecutable, paridad de backend,
prueba negativa cuando corresponda, documentación y publicación sincronizada.

## 240. Igualdad estructural dentro de la IR nativa — 2026-09-21

La semántica de `==`/`!=` para colecciones y wrappers ya no obliga al backend nativo a abandonar
la IR cuando aparece en una función con CFG:

- `ir_c` acepta ahora comparaciones estructurales de `List`, `Map`, `Set`, `Option` y `Result`
  cuando sus payloads tienen una representación soportada. El callback de helpers reutiliza los
  mismos `ostrin_eq_*` que el emisor C existente, por lo que los valores anidados, records con
  `derive(Eq)` y strings conservan una sola implementación de igualdad;
- el constructor `None` quedó cubierto en la misma ruta de llamadas IR, evitando que una opción
  vacía fuerce el fallback aunque se compare dentro del CFG;
- `examples/structural_equality.ostrin` conserva siete resultados idénticos entre intérprete y
  nativo, exige `ir-generated: 1` y ejecuta el binario con `--leak-check`, que termina en
  `live_allocations=0`.

La semántica de `Array` sigue siendo deliberadamente elemento a elemento y devuelve una máscara;
no se mezcla con la igualdad booleana estructural de las colecciones. Los consumidores complejos
que aún no tienen representación IR permanecen en el fallback verificado.

## 241. `std.args` y `std.env` — 2026-09-21

La biblioteca estándar ya cubre también la frontera de proceso y filesystem sin duplicar el
runtime por backend:

- `std.args.all()` devuelve los argumentos del programa (`--` los separa de las opciones de
  `--run` en el intérprete), `count()` expone su cardinalidad y `at()` devuelve `Option<String>`
  con límites seguros;
- `std.env.get()` consulta una variable como `Option<String>`, mientras `current_dir()`, `join()` y
  `exists()` delegan en los builtins portables de directorio, rutas y existencia de archivos;
- `examples/std_args_env.ostrin` ejecuta la misma fuente con `uno dos` y `OSTRIN_TEST_VALUE` en
  intérprete y nativo. El binario nativo se compila con `--leak-check` y termina en cero
  asignaciones vivas.

El registro remoto y la red siguen deliberadamente fuera de este bloque: estos módulos sólo
estabilizan APIs locales ya implementadas y no introducen acceso externo implícito.

## 242. Builtins de proceso y rutas dentro de la IR nativa — 2026-09-21

La superficie que `std.args` y `std.env` delegan ya no fuerza por sí misma el fallback HIR/AST:

- `ir_c` baja `args()` como una `List<String>` fresca, `env()` como `Option<String>` con una
  copia gestionada del valor de `getenv`, y `cwd()`, `path_join()` y `file_exists()` con sus
  helpers C existentes;
- `clone` y `drop` tienen lowering explícito para familias gestionadas, de modo que
  `std.args.at()` puede clonar un argumento antes de liberar la lista temporal y `count()` puede
  consumirla sin fuga;
- `examples/std_args_env.ostrin` exige `ir-generated: 8`, `hir-generated: 0`, cubre índices fuera
  de rango por ambos extremos, compara salida con el intérprete y termina con
  `live_allocations=0`.

El bloque no convierte todavía `read_file`/`write_file` en E/S cancelable: esas operaciones
externas bloqueantes siguen siendo la siguiente frontera de concurrencia y requieren un contrato
de runtime separado, especialmente para WASI y `--native-threads`.

## 243. E/S de archivos en la IR nativa — 2026-09-21

La primera ampliación de esa frontera ya está cerrada sin prometer una interrupción que el runtime
no puede garantizar:

- `ir_c` representa ahora `Result<Void, String>` además de `Result<String, String>`, por lo que
  `write_file` y `read_file` pueden cruzar HIR→IR→C en funciones con CFG. El ejemplo
  `examples/native_ir_file_io.ostrin` exige tres funciones generadas desde IR y cero desde HIR.
- El lowering nativo hace un checkpoint antes de llamar a la libc, conserva el `Result` por valor y
  duplica cada texto de error a una asignación administrada. `read_file` valida `fseek`, `ftell`,
  lectura corta, `ferror` y `fclose`; `write_file` valida `fputs` y `fclose` en lugar de declarar
  éxito después de abrir el archivo.
- La regresión compara intérprete y C, usa `--leak-check` y termina con `live_allocations=0`.
  La misma implementación sigue siendo válida para WASI porque usa la interfaz C de archivos del
  host, pero los mensajes derivados de `strerror` pueden variar entre plataformas.

El contrato de concurrencia queda explícito: una cancelación solicitada antes de la llamada se
observa en el checkpoint; una cancelación durante `fopen`/`fread`/`fputs`/`fclose` espera a que la
libc regrese y se observa en el siguiente punto seguro. No se presenta esta entrega como E/S
asíncrona ni como preempción de un hilo bloqueado; separar esa E/S en workers o un mecanismo WASI
es la siguiente decisión de runtime.

## 244. `std.maps` sobre el `Map<K,V>` incorporado — 2026-09-21

La biblioteca estándar ya ofrece una superficie pequeña y reutilizable para consultas de mapas,
sin introducir una segunda implementación de `HashMap`:

- `compiler/std/maps.ostrin` expone `count`, `is_empty`, `contains_key`, `get_or`, `keys` y
  `values`. Sus funciones son genéricas y exigen `Hash + Eq` sólo para la clave, de modo que
  sirven para valores escalares y para valores gestionados sin duplicar las reglas del checker.
- `get_or` compone `Map.get(...).unwrap_or(...)`; `keys` y `values` devuelven listas nuevas y
  no mutan el mapa. La mutación permanece en la API incorporada (`mut`, `.set`, `.remove`), lo
  que evita esconder transferencias de ownership en helpers de alto nivel.
- `examples/std_library.ostrin` y `examples/std_tests.ostrin` cubren presencia/ausencia,
  fallback, cardinalidad y extracción de colecciones. La prueba de integración compara la
  salida del intérprete y del C nativo y compila con `--leak-check`; las listas temporales se
  descartan explícitamente y el reporte termina en `live_allocations=0`.

El registro remoto y la red siguen fuera de este bloque: `std.maps` sólo estabiliza una API local
sobre el mapa que ya existe en ambos backends.

## 245. Formateo decimal configurable en `std.strings` — 2026-09-21

La biblioteca estándar cubre ahora una necesidad frecuente de salidas científicas y de reportes:

- `std.strings.format_float(value, digits)` devuelve `Result<String, String>` y utiliza notación
  decimal fija con una precisión explícita de 0 a 18 cifras. Fuera de ese intervalo devuelve un
  `Err("float precision must be between 0 and 18")`, de forma que el llamador puede usar `try`,
  `unwrap` o las consultas normales de `Result`.
- El intérprete usa el formateo dinámico de `f64`; el backend C comparte el contrato mediante
  `ostrin_float_format`, y tanto la ruta HIR/C como la IR/C generan el mismo `Result<String,String>`.
  Los errores de precisión son literales estáticos, evitando una reserva gestionada cuando sólo se
  consulta `is_err()`.
- La regresión `std_library_modules_agree_between_backends_and_pass_their_own_tests` compara
  `3.14`, `-0.125`, precisión cero y precisiones inválidas, verifica que el símbolo llegue al C
  emitido y termina el binario con `live_allocations=0`. El playground mantiene la misma fuente
  de ejemplo mediante el check de deriva del sitio.

## 246. E/S de archivos cancelable en tareas nativas — 2026-09-21

La frontera de concurrencia de archivos ya tiene un contrato ejecutable para `--native-threads`,
sin afirmar preempción de llamadas C:

- `compiler/src/file_io_runtime.c` conserva la operación bloqueante (`fopen`, `fread`, `fputs` o
  `fclose`) en un worker nativo desacoplado. La tarea espera la solicitud mediante una condición
  temporizada y revisa `ostrin_cancellation_requested()` en cada vuelta.
- La solicitud tiene referencias separadas para la tarea y el worker. Si `Task.cancel()` llega
  mientras la tarea espera, la tarea libera su referencia antes de saltar al checkpoint; el worker
  conserva la suya, termina la libc y libera path, contenido, resultado y condición sin tocar
  memoria administrada por Ostrin. Sólo el resultado que vuelve a una tarea viva se duplica a una
  `String` rastreada por el runtime.
- HIR/C e IR/C llaman los mismos helpers `ostrin_file_read_cancelable` y
  `ostrin_file_write_cancelable`. `examples/native_ir_file_io.ostrin` ahora compara ambos modos,
  exige la presencia del worker cancelable en el C emitido y mantiene `live_allocations=0`.
- El runtime cooperativo y `wasm32-wasip1` permanecen síncronos: no se introducen pthreads ni una
  falsa promesa de interrupción del host WASI. En ningún modo se fuerza la terminación de una
  llamada libc en curso; en nativo sólo se desacopla la espera de la tarea y se garantiza la
  limpieza posterior del worker.

Quedan fuera de este cierre la E/S de red, un pool global de workers y una política de apagado que
  espere operaciones de larga duración al finalizar el proceso. Esas decisiones requieren un
  contrato de recursos más amplio que el de archivos locales.

## 247. Consumidores gestionados de `Option`/`Result` desde la IR — 2026-09-21

La migración nativa ya no deja sin contrato de ownership a los consumidores estructurales que
extraen un payload gestionado:

- `Option<T>.unwrap()` y `Result<T,E>.unwrap()` copian el payload activo a un temporal C y lo
  retienen antes de que la IR libere el wrapper. `unwrap_or` aplica la misma regla tanto al valor
  activo como al fallback; la rama seleccionada se evalúa una sola vez desde sus valores SSA.
- `Option<T>.ok_or(E)` construye el `Result<T,E>` por valor y retiene el payload de `Some` o el
  error elegido en la rama `None`. `Result<T,E>.ok()` ya tenía la retención de `Some`, y ahora el
  análisis de último uso reconoce ambos lados como sitios seguros de liberación.
- La expansión es recursiva para los tipos que el emisor ya representa (`String`, listas/mapas/
  conjuntos soportados, records concretos y wrappers anidados). Los payloads escalares siguen
  usando la expresión compacta sin coste de referencia.

`examples/native_ir_managed_consumers.ostrin` cubre `unwrap`, `unwrap_or`, `ok_or` y `ok` en
éxito y error para `Option<String>` y `Result<String,String>`. La prueba compara intérprete y C,
exige al menos diez funciones `ir-generated`, inspecciona el C emitido y ejecuta con
`--leak-check`, terminando en `live_allocations=0`.

Este bloque reduce otra fuente concreta de fallback y cierra el contrato de extracción; todavía
quedan consumidores complejos no lineales, scopes/escapes y tipos compuestos que no tienen un
protocolo completo en IR/C.

## 248. Matriz de programas WASI y contratos de plataforma — 2026-09-21

El workflow WASI deja de probar sólo que el compilador y dos programas se puedan enlazar:

- `scripts/wasi-program-check.mjs` construye con el compilador host cinco módulos
  `wasm32-wasi`: `hello`, el proyecto con dependencia `path`, `wasi_io_contract`,
  `native_ir_file_io` y `native_ir_managed_consumers`.
- Cada módulo se ejecuta bajo Node WASI con `stdout` y `stderr` capturados en descriptores
  explícitos. La regresión compara salidas completas, argumentos `alpha`/`beta`, la variable
  `OSTRIN_WASI_TEST`, creación/lectura de un archivo preabierto y los resultados/ownership de
  `Option`/`Result`; no se reduce a buscar una línea parcial.
- La matriz limpia sus archivos de prueba, incluye todos los módulos en `SHA256SUMS` y en el
  tarball WASI. El workflow compila además el `ostrinc` host una sola vez para que la matriz use
  exactamente el mismo CLI que el usuario local.
- `wasm_program_matrix_emits_without_native_thread_dependencies` verifica localmente la emisión
  cooperativa de los programas y del proyecto de paquetes, y rechaza cualquier
  `OSTRIN_NATIVE_THREADS`. La ejecución final bajo Node WASI queda cubierta por el workflow con
  el SDK C fijado y checksum verificado.

Se mantiene deliberadamente fuera de WASI la interrupción de libc y cualquier pthread; el contrato
actual es cooperativo, con archivos y entorno provistos por los preopens/host WASI. La siguiente
frontera de plataforma sigue siendo ampliar recursos soportados sin mezclar semántica nativa de
hilos con el target WASI.

## 249. Fuzzing diferencial de wrappers gestionados — 2026-09-21

El generador diferencial ya cubre una familia independiente de programas con ownership, no sólo
la regresión manual:

- `ManagedWrapperGen` produce entre tres y cinco casos por semilla, con ramas deterministas de
  `Some`/`None` y `Ok`/`Err`, textos distintos y funciones que reciben `Option<String>` o
  `Result<String, String>` como parámetros.
- `generated_managed_wrappers_agree_between_interpreter_and_native_backend` ejecuta cada programa
  con el intérprete y el backend C, compara stdout completo y exige que el binario nativo termine
  con `live_allocations=0`. Cubre `unwrap`, `unwrap_or`, `ok`, `ok_or`, `match` y los fallbacks
  tanto en éxito como en error; las operaciones inseguras (`unwrap` sobre ausencia) sólo aparecen
  en la rama que la semilla marca como válida.
- Usa el mismo `OSTRIN_FUZZ_SEEDS` que el generador escalar: cuatro semillas por defecto y 25
  semillas verificadas en esta sesión. Así la ampliación de búsqueda no cambia el contrato básico
  de CI, pero deja una reproducción sencilla para regresiones de ownership.

La suite queda en **6 pruebas diferenciales, 192 de integración y 2 unitarias**. El siguiente
trabajo de calidad sigue siendo cubrir diagnósticos de error de forma sistemática y añadir
benchmarks comparables entre intérprete y backend nativo; los consumidores complejos no lineales,
scopes y escapes siguen fuera del protocolo IR/C.

## 250. Instancias genéricas concretas desde IR/C — 2026-09-22

Las instancias monomorfizadas de funciones y métodos ya no se detienen automáticamente en HIR:
después de aplicar los tipos concretos, el backend intenta bajar cada cuerpo a IR, ejecutar el
análisis lineal de ownership y generar C. Cuando la IR o el emisor no pueden representar el cuerpo,
se conserva la ruta HIR y, donde ya correspondía, el fallback AST.

- `ir_c` ahora recibe un mapa de nombre lógico a símbolo C en vez de asumir que ambos coinciden
  tras aplicar `c_function_name`. Esto permite que las llamadas ordinarias con prefijo y las
  llamadas a instancias monomorfizadas/recursivas resuelvan al símbolo concreto correcto.
- `native_hir_generics.ostrin` verifica seis funciones IR y cero HIR, incluyendo llamadas
  genéricas concretas y recursión. Compara salida del intérprete y del binario y exige
  `live_allocations=0`.
- `native_generic_methods.ostrin` verifica cinco funciones IR y una función HIR: el retorno del
  record genérico de ese caso sigue fuera de la representación IR disponible. También exige
  paridad de salida y cero asignaciones vivas.
- Los records y enums genéricos aplicados no se declaran migrados por este bloque: sus fixtures
  mantienen el fallback ya existente. Esta separación queda reflejada en `--native-type-report`
  y en las aserciones de regresión.

Verificación de esta continuación: `cargo test --manifest-path compiler/Cargo.toml` pasó con
2 pruebas unitarias, 6 diferenciales y 192 de integración. El build del compilador para
`wasm32-wasip1 --release` también pasó. Se intentó la matriz end-to-end
`node scripts/wasi-program-check.mjs`, pero este entorno no tiene configurado `OSTRIN_WASI_CC`
ni un compilador C de WASI; por eso no se pudo ejecutar aquí y queda cubierta por el workflow
WASI fijado en `.github/workflows/wasi.yml`.

## 251. WASI SDK 34 y portabilidad de punteros de 32 bits — 2026-09-22

Se cerró la brecha entre el workflow fijado y las verificaciones locales: se instaló WASI SDK
34.0 para Windows x64, se comprobó su SHA-256 oficial y se configuraron `OSTRIN_WASI_CC` y
`OSTRIN_WASI_SYSROOT` para reproducir la matriz sin instalar herramientas globales del sistema.

- Ostrin mantiene `--target wasm32-wasi` como nombre compatible de CLI, pero ahora solicita a
  Clang el triple actual `wasm32-wasip1`. El runtime de cancelación cooperativa usa
  `setjmp`/`longjmp`; por ello se habilita SJLJ con `-mllvm -wasm-enable-sjlj` y los programas
  resultantes requieren un host WASI Preview 1 con soporte de WebAssembly exception handling.
- La matriz compila con `-Werror=shift-count-overflow`. Ese control reveló que el hash de punteros
  del registro de asignaciones desplazaba 33 bits sobre `uintptr_t`, también de 32 bits en WASI.
  El runtime usa ahora mezclas distintas según `UINTPTR_MAX`, sin desplazamientos inválidos.
- El workflow y la guía de distribución declaran explícitamente el requisito del host; `/dist/`
  queda ignorado como salida generada de la matriz, sin mezclar los módulos de prueba con fuentes.

Verificación de este bloque: `cargo test --manifest-path compiler/Cargo.toml` pasó con 2 pruebas
unitarias, 6 diferenciales y 192 de integración; `node scripts/wasi-program-check.mjs` compiló y
ejecutó los cinco módulos con salidas exactas y sin warnings de desplazamiento; `ostrinc.wasm`
compiló para `wasm32-wasip1` y pasó `--check examples/hello.ostrin` bajo Node WASI; también
pasaron `node scripts/website-check.mjs --wasm`, `node scripts/distribution-check.mjs` y
`node scripts/website-check.mjs`.

## 252. Hechos públicos del sitio comprobados contra el repositorio — 2026-09-22

La auditoría de descubrimiento ya no depende de cifras copiadas a mano que puedan envejecer en
silencio. Se alinearon las páginas estáticas con el estado comprobado del compilador:

- 195 fuentes `.ostrin`, 22 documentos de diseño, 192 pruebas de integración, 6 diferenciales,
  2 unitarias y versión `0.1.0`.
- `scripts/website-check.mjs` ahora obtiene versión desde `compiler/Cargo.toml`, cuenta archivos
  fuente y documentos, y cuenta atributos `#[test]` de los tres módulos de pruebas.
- El check compara el resultado con `SITE_FACTS`, todos los fallbacks HTML, el README, la
  roadmap y la auditoría actual. Ya corre en CI y en el workflow Pages; una cifra nueva sin
  actualizar el sitio detiene la publicación.
- Se sustituyeron las etiquetas HTML `prototype / 0.1` por el fallback `development / 0.1.0` y
  se reescribió `docs/website-audit.md` como inventario actual, retirando afirmaciones que ya
  habían quedado atrás sobre el playground, SEO, showcase, comunidad y el tamaño de la suite.

Verificación de este bloque: `node scripts/website-check.mjs --wasm` pasó e informó 9 páginas
públicas, 195 ejemplos y 192 pruebas de integración; `node scripts/distribution-check.mjs` y
`git diff --check` también pasaron. El bloque previo corrió la suite completa del compilador.

## 253. Consumidores lineales de wrappers anidados en IR/C y WASI — 2026-09-22

El ownership recursivo de constructores ya existía; ahora se verifica también cuando el programa
extrae payloads por cadenas de métodos y atraviesa ramas de fallback:

- `examples/native_ir_nested_wrappers.ostrin` cubre `unwrap`, `unwrap_or`, `ok` y `ok_or` sobre
  `Option<Option<String>>` y `Result<Option<String>,String>`, tanto en rutas presentes como en
  fallbacks. La prueba requiere al menos diez funciones `ir-generated`, cero `hir-generated`,
  igualdad exacta entre intérprete y C, y `live_allocations=0`.
- El mismo ejemplo se añadió a `scripts/wasi-program-check.mjs`. La matriz ahora compila y ejecuta
  seis módulos con stdout exacto; workflow, README del artefacto, SHA-256 y tarball incluyen el
  módulo nuevo. `distribution-check.mjs` protege su entrada en lista, checksum y archivo.
- Se acotaron las notas de implementación: consumidores anidados lineales sobre tipos soportados ya
  están en IR/C; handlers capturados, consumidores no lineales, patrones anidados y escapes
  complejos permanecen como fronteras abiertas.

Verificación de este bloque: `cargo test --manifest-path compiler/Cargo.toml` pasó con 2 pruebas
unitarias, 6 diferenciales y 192 de integración; `node scripts/wasi-program-check.mjs` ejecutó
correctamente los seis módulos bajo Node WASI sin warnings de shifts; el test nativo exigió
`hir-generated: 0` y `live_allocations=0`; `node scripts/distribution-check.mjs` pasó.

## 254. Tarjeta social del sitio y metadatos OG/Twitter — 2026-09-22

Se reemplazó el logo cuadrado como vista previa social por una pieza horizontal propia para
Ostrin: fondo técnico violeta discreto, marca oficial sin alteraciones y texto alineado con el
posicionamiento actual de la portada. La tarjeta mide 1200×630 y mantiene frases comprobables
del sitio: lenguaje científico-first y general-purpose, backend nativo C/WASI y licencia MIT.

- Las nueve páginas públicas usan el mismo PNG absoluto para Open Graph y Twitter/X, con tarjeta
  grande, tipo, dimensiones, descripción alternativa y títulos/descripciones sincronizados.
- `scripts/website-check.mjs` valida la firma e IHDR del PNG, sus dimensiones exactas y todos los
  metadatos sociales por página; la comprobación existente protege ese contrato en CI y Pages.
- Se conserva `website/assets/ostrin-logo.png`; el arte social es un recurso separado y no cambia
  el playground, WASM ni la identidad visual existente.

Verificación de este bloque: `node scripts/website-check.mjs --wasm` pasó para las nueve páginas,
195 ejemplos, 192 pruebas de integración y el artefacto WASM; `node scripts/distribution-check.mjs`
y `git diff --check` también pasaron.

## 255. Regresiones de navegador para WASM, tablet y móvil — 2026-09-22

La cobertura del sitio deja de limitarse a referencias estáticas: la nueva suite Chromium compila
el estado actual del compilador a WASM, sirve el sitio bajo el prefijo real `/Ostrin/` y comprueba
el recorrido de aprendizaje junto con su ejecución en navegador.

- `tests/browser/` aísla Playwright como dependencia de desarrollo fijada y guarda lockfile propio.
  `npm test` reconstruye `wasm32-wasip1` desde fuentes actuales antes de levantar un servidor local
  con tipos MIME correctos y rutas cercadas al sitio.
- La suite ejecuta el ejemplo de cantidades real y exige `5 m/s`; reemplaza el editor por una
  expresión con tipos incompatibles y comprueba el diagnóstico y su línea en el editor. También
  verifica página/cabeceras y que no aparezcan errores JavaScript.
- El menú responsive ahora está disponible hasta 1000 px: Chromium Linux detectó desbordamiento
  de 988 px en una ventana de 981 px. La rejilla de ejemplos pasa a dos columnas en tablet tras
  medir 795 px de ancho total a 768 px. A 390 px, la documentación alcanzaba 592 px por el mínimo
  intrínseco de las columnas del grid; las columnas colapsadas usan ahora `minmax(0, 1fr)`.
- La regresión revisa las nueve páginas en 390 y 768 px, además de la portada desktop. `ci.yml` y
  Pages reutilizan `.github/actions/website-verify`, que compila/prueba el playground y ejecuta los
  contratos estáticos antes del CI verde o la carga del artefacto de publicación.
- La auditoría aclara lo que queda fuera: otros motores (Firefox/WebKit), lector de pantalla y
  baselines visuales completos.

Verificación local de este bloque: `npm ci` reconstruyó las tres dependencias fijadas;
`npm test` pasó con 4 pruebas —compilador/diagnóstico reales, menú a 390/768 px, las nueve páginas
en ambos viewports y el cambio del encabezado entre 1000 y 1001 px sin desbordamiento—;
`cargo test --manifest-path compiler/Cargo.toml` pasó con 2 unitarias,
6 diferenciales y 192 de integración; `node scripts/website-check.mjs --wasm`,
`node scripts/distribution-check.mjs` y `git diff --check` también pasaron. CI y Pages reutilizan
la acción compartida para repetir estas comprobaciones en GitHub antes de publicar.

## 256. E1101 sensible al CFG y a Phi — 2026-09-22

El chequeo de movimientos ya no interpreta `function.blocks` como si el orden almacenado fuera el
orden de ejecución. `check_moves_impl` ahora propaga los valores movidos por las aristas alcanzables
del CFG con un worklist de punto fijo; por ello una rama mutuamente excluyente no contamina la otra,
los joins conservan el riesgo de cualquier predecesor, y las iteraciones/backedges se analizan hasta
converger. Cada entrada `Phi` se comprueba sólo en la arista que la selecciona y un alias `Phi` de un
valor movido mantiene ese estado después del merge. La guarda dinámica permanece activa.

- Se añadieron tres pruebas: ramas exclusivas válidas, selección de `Phi` por arista, y rechazos
  tras joins, en condición/reenvío de loops y en alias `Phi` de un valor movido. Se exige además que
  intérprete y backend nativo rechacen E1101 por sus entry points habituales.
- Se sincronizaron a 195 los contadores actuales de tests en README, portada, ejemplos, roadmap y
  auditoría pública del sitio.

Verificación local: `cargo test --manifest-path compiler/Cargo.toml` pasó con 2 pruebas unitarias,
6 diferenciales y 195 de integración; `npm test` pasó las cuatro regresiones Chromium;
`node scripts/website-check.mjs --wasm` confirmó 9 páginas, 195 ejemplos y 195 pruebas;
`node scripts/wasi-program-check.mjs` compiló y ejecutó los seis módulos con salida exacta;
`node scripts/distribution-check.mjs` y `git diff --check` también pasaron. La sesión tuvo que
cargar explícitamente las variables del SDK WASI 34 ya instalado en el perfil de usuario.

## 257. E1101 dinámico sigue el lifetime del record — 2026-09-22

Una prueba de churn reprodujo una falla real de las guardas dinámicas: `--run` enviaba y destruía
un record dentro de una función, el allocator reciclaba su dirección y el siguiente record nuevo
era rechazado como movido al enviarlo. El mismo diseño tenía otra brecha: la tabla global C
acumulaba direcciones sin límite y asumía incorrectamente que los records nunca se liberaban.

- El intérprete guarda `Weak` al identity de cada record movido, conserva el estado mientras el
  objeto sigue vivo y purga entradas expiradas al consultar o en barridos geométricos durante nuevos
  envíos; la referencia débil impide ABA de dirección sin retener el objeto.
- El runtime C ahora lleva el bit `moved` en la entrada del allocation gestionado. Marcar y leer
  pasan por el mutex de allocations; al liberar el record, la entrada —y el bit— desaparecen juntos.
  Se elimina la tabla paralela de direcciones que podía crecer y quedar obsoleta.
- La regresión crea/consume records en 2048 iteraciones dentro de dos tareas, compara el intérprete
  cooperativo con nativo cooperativo y `--native-threads`, y requiere `live_allocations=0` en ambos
  ejecutables.

Verificación de este bloque: `cargo test --manifest-path compiler/Cargo.toml` pasó con 2 unitarias,
6 diferenciales y 196 de integración; `npm test` pasó las cuatro pruebas Playwright sobre el
compilador WASM real; `node scripts/website-check.mjs --wasm` validó 9 páginas, 195 ejemplos y
196 pruebas; `node scripts/wasi-program-check.mjs` compiló y ejecutó los seis programas, y
`node scripts/distribution-check.mjs` validó matriz de release, checksums e instaladores. No hizo
falta instalar dependencias; se cargaron en la sesión las variables del SDK WASI 34 ya instalado.

## 258. Transferencia real de records mutables por canal — 2026-09-23

La auditoría posterior encontró una incoherencia: E1101 invalidaba correctamente el binding del
emisor, pero la guarda dinámica también bloqueaba al receptor después de `receive()`, y el camino
AST con hilos nativos dejaba viva la referencia transferida dentro de un patrón `Some(buffer)`.

- El runtime marca el record mientras está en vuelo por el canal y limpia el bit al extraerlo por
  `receive()` o `select`; el receptor puede leer el record transferido, sin revalidar una identidad
  que ya recuperó ownership.
- El emisor conserva la prohibición estática, incluidos aliases; recibir el valor no rehabilita
  bindings que quedaron en la tarea emisora.
- Los bindings de payload en patrones `Some`/`Ok`/`Err` entran en un frame de cleanup del brazo,
  de modo que el receptor libera exactamente la referencia transferida en intérprete, IR y AST.
- Se añadió una regresión que verifica intérprete, nativo IR, AST con `--native-threads`, leak-check
  y el rechazo E1101 de un alias emisor.

Verificación de este bloque: `cargo test --manifest-path compiler/Cargo.toml` pasó con 2 unitarias,
6 diferenciales y 197 de integración; `npm test` pasó 4/4 pruebas Playwright; `website-check.mjs`
validó 9 páginas y 197 pruebas; la matriz WASI de 6 programas, distribución, checksums e
instaladores también pasaron.

## 259. Valores de función nombrada y llamadas indirectas en IR/C — 2026-09-23

La frontera entre HIR y la IR todavía rechazaba cualquier función que apareciera como valor:
una llamada como `operation(value)` se convertía en `Opaque`, aunque el emisor HIR ya tenía
un ABI de closure para funciones globales sin entorno. Se cerró esa inconsistencia para la forma
sin captura:

- La IR distingue `ClosureCall` de una llamada estática. El lowering conserva el valor callee,
  y el emisor C valida la firma completa, prepara el `OstrinClosure` y llama mediante su puntero
  con `env == NULL`.
- Cada función global usada como valor recibe un adaptador C con la firma `(void*, args...)`;
  así la representación es portable dentro del ABI de closures y no depende de invocar una
  función C ordinaria con una convención incompatible.
- Los valores de función pueden copiarse a locales, pasarse como parámetros y devolverse desde
  llamadas indirectas dentro de la IR. Las lambdas capturadas y sus entornos siguen en HIR hasta
  que el análisis de lifetime del entorno tenga una representación explícita en IR.
- `examples/native_ir_function_values.ostrin` cubre llamada local y paso a `apply`. La regresión
  exige tres funciones `ir-generated`, cero `hir-generated`, paridad exacta y `live_allocations=0`.

Verificación de este bloque: el ejemplo pasa en intérprete, `--native-type-report` y binario C con
`--leak-check`; la prueba dirigida pasa con 198 integraciones. La matriz WASI se amplió a siete
programas y ejecutó el nuevo módulo bajo Node WASI con salida exacta; `website-check.mjs` y
`distribution-check.mjs` también pasan. Se actualizaron los contadores públicos a 196 programas
fuente y 198 pruebas de integración.

## 260. Closures capturadas con entorno explícito en IR/C — 2026-09-23

El bloque anterior ya había llevado las funciones nombradas a `ClosureCall`, pero una lambda que
capturaba un binding todavía se representaba como `Opaque` y forzaba el fallback HIR. Se cerró la
siguiente pieza del ABI de valores de función:

- `IrInstr::ClosureMake` conserva los `ValueId` capturados y cada lambda compatible se convierte en
  una `IrClosure` auxiliar con parámetros de entorno más parámetros de llamada. La función auxiliar
  usa el mismo emisor CFG que una función normal, por lo que sus retornos y llamadas también pasan
  por la verificación de tipos del backend IR/C.
- El emisor genera una estructura C de entorno por closure, un adaptador `(void*, args...)` y un
  destructor registrado. Las capturas gestionadas se retienen al construir el entorno y se liberan
  cuando el último `ClosureCall`/`release` descarta el valor de función; los retornos gestionados de
  la lambda retienen el payload cuando proceden de un parámetro capturado.
- `examples/native_hir_closures.ostrin` ahora ejercita una captura escalar y una captura `String`
  materializada, y exige cero `hir-generated`, paridad exacta y `live_allocations=0`.

Las closures anidadas con capturas transitivas ya comparten esa representación: el escaneo de
nombres libres propaga el valor desde el entorno exterior, y el helper IR anidado genera su propio
entorno C con retain/release recursivo. La frontera restante son cuerpos no lineales con scopes o
escapes complejos, handlers locales no inline y el análisis completo de ownership para esos casos;
siguen cayendo de forma verificable a HIR/AST.

## 262. Capturas transitivas en closures anidadas — 2026-09-23

El primer lowering de `ClosureMake` rechazaba cualquier lambda cuyo cuerpo contuviera otra lambda.
Eso dejaba sin migrar una forma esencial de funciones de orden superior: una función que devuelve
un closure que a su vez devuelve otro closure, como `make_scaler` o un factory que conserva un
`String` del entorno exterior.

- `collect_lambda_locals_expr` ahora recorre una lambda anidada con su propio conjunto de parámetros
  ligados y propaga sus nombres libres al escaneo de la lambda envolvente. El entorno exterior
  recibe los valores que vienen de la función padre; el lowering de la lambda exterior los vuelve a
  encontrar al construir el helper interior.
- Se retiró el veto global `hir_contains_lambda`: cada cuerpo continúa pasando por el mismo lowering
  HIR→IR/ownership y `build_closure_helpers` genera recursivamente los adaptadores y destructores
  C de los helpers anidados.
- `examples/native_hir_closures.ostrin` ahora cubre una captura transitiva escalar (`k`, `a`) y
  otra gestionada (`prefix: String`) con llamadas encadenadas. La prueba exige paridad exacta,
  `ir-generated: 4`, cero HIR fallback y `live_allocations=0`.

Verificación dirigida de este bloque: `--native-type-report` informa 4 funciones IR y 0 HIR;
`--ir` informa 0 instrucciones opacas, 0 bloques sin terminador y 0 violaciones; el ejemplo
interpreta y compila nativamente con salida `15`, `value!`, `5`, `23`, `nested ` y leak-check en
cero. La suite completa del compilador y los gates web/WASM siguen siendo obligatorios antes del
push.

## 261. Metadata de la web generada desde el repositorio — 2026-09-23

La auditoría de la Fase B encontraba que la web ya tenía validadores para detectar cifras obsoletas,
pero todavía guardaba manualmente la versión y los contadores en `website/site.js` y en los fallbacks
HTML. Eso dejaba dos fuentes de verdad para cada cambio del compilador.

- `scripts/site-facts.mjs` calcula la versión de `compiler/Cargo.toml`, los documentos de diseño,
  los programas `.ostrin` y los tests Rust desde el árbol actual.
- `scripts/website-metadata.mjs --write` genera `website/site-data.js`; sin `--write` comprueba que
  el artefacto versionado coincide exactamente con esas fuentes. Todas las páginas públicas cargan
  ese archivo antes de `site.js`; los placeholders HTML ya no contienen números ni versiones
  duplicados.
- La acción común de CI/Pages ejecuta la comprobación antes del chequeo estático, de modo que una
  publicación no puede avanzar con métricas de sitio desactualizadas.

Verificación de este bloque: `node scripts/website-metadata.mjs`, `node scripts/website-check.mjs`
y la suite browser/WASM de la acción deben pasar; la identidad visual, el playground WASM real,
los enlaces y las páginas existentes se conservan.

## 263. Handlers locales capturados en `try catch` — 2026-09-23

El lowering de `try ... catch` ya resolvía handlers globales y aliases locales de funciones globales,
pero un alias cuyo valor era una closure capturada no emitía una llamada: recuperaba el objeto de
closure como si fuera el error transformado. La ruta nativa quedaba fuera justo en una combinación
importante de `Result`, funciones como valores y ownership de entornos.

- `lower_try` conserva la llamada estática para handlers globales y ahora emite `ClosureCall` para
  handlers locales o expresiones de handler que produzcan una closure compatible.
- El entorno tipado de la closure se mantiene como fuente de captura; la rama `catch` pasa el error
  como argumento y construye el `Err` de retorno después de la transformación, sin copiar el
  objeto closure como payload.
- `examples/native_ir_try_captured_handler.ostrin` crea un handler que captura un prefijo `String`
  y lo usa desde dos funciones. La regresión compara intérprete y C, exige `hir-generated: 0` y
  `live_allocations=0`.

Verificación local del bloque: `cargo test` pasó 2 unitarias, 6 diferenciales y 199 de integración;
la matriz WASI de ocho programas compiló y ejecutó el nuevo caso con salida exacta;
`node scripts/website-metadata.mjs --write` actualizó los contadores públicos a 197 programas y
199 pruebas. Los handlers con cuerpos no lineales, scopes/escapes complejos y payloads todavía no
representados conservan el fallback verificado.

## 264. Iteradores genéricos monomorfizados en IR/C — 2026-09-23

El hueco que quedaba después de migrar los iteradores de records concretos era la pérdida de
la sustitución de tipos en `impl<T> Iterator<T> for Cursor<T>`. El checker conocía
`Cursor<Int>`, pero la metadata HIR sólo guardaba `Cursor -> T`, por lo que el `for` terminaba
en los nodos legacy `iter_init`/`iter_next` y el backend generaba HIR/AST.

- `HirProgram` conserva ahora el patrón de argumentos del receiver y el elemento de cada
  implementación `Iterator`; el lowering IR unifica patrones anidados y sustituye `T` por el
  argumento concreto antes de construir el polling `Option<T>`.
- `ir_c.rs` reconoce las instancias de records aplicados (`Cursor<Int>` → `Cursor__Int`) en
  tipos, agregados, campos, métodos y ownership. La asignación de campos escalares necesaria
  para `next(mut self)` tiene una instrucción IR explícita `FieldStore`; los campos gestionados
  conservan fallback hasta que exista metadata de tipos de campo suficiente para actualizar
  ownership de forma segura.
- `examples/native_generic_iterator.ostrin` ejecuta tres valores `7`, exige dos funciones
  `ir-generated`, cero `hir-generated`, cero divergencias, llamada `Cursor__Int__next` y
  `live_allocations=0`. La matriz WASI incorpora el mismo programa con salida exacta.

Verificación local del bloque: la regresión dirigida pasó; la suite completa debe mantener 2
unitarias, 6 diferenciales y 200 de integración. `website/site-data.js` se regenera a 198
ejemplos y 200 pruebas; el script WASI queda en nueve programas. Los iteradores indirectos,
scopes complejos, agregados genéricos con payloads gestionados y la retirada total del fallback
siguen pendientes.

## 265. Prueba concurrente con orden parcial — 2026-09-23

CI de `36abf4c` falló en Ubuntu por una expectativa de orden total en
`native_threads_scope_drain_releases_nested_task_handles`. El programa permite dos pares
concurrentes: `main`/`task` y `scope-body`/`scope-task`. La prueba exige exactamente seis
líneas, compara cada par sin imponer orden y mantiene `42` después de `join()` y `7`
después del drenado del scope. Se conserva la comprobación `live_allocations=0`.

El bloque anterior pasó localmente 2 unitarias, 6 diferenciales y 200 de integración.
Después de la corrección, CI y Pages pasaron en Windows, Linux, macOS y web para `8bc24bd`.

## 266. Consolidación previa a la release `v0.1.0` — 2026-09-23

Auditoría de coherencia antes de publicar el tag. README, `ESTADO_Y_PLAN.md`, `docs.html` y
`roadmap.html` afirmaban una release `v0.1.0` publicada e instaladores verificados contra ella,
pero el repositorio no tenía tags ni releases y los workflows de release y WASI nunca se habían
ejecutado. Los textos distinguen ahora «versión 0.1.0 preparada» de «release publicada», y
`ESTADO_Y_PLAN.md` añade una tabla §0 con implementado / en fallback / experimental /
pendiente / release.

- `scripts/install.ps1`: `-not $Repository -match …` negaba la cadena antes de comparar, así
  que nunca rechazaba un `-Repository` inválido; ahora usa `-notmatch`.
- `release.yml`: la release usa `docs/releases/<tag>.md` como notas, `--verify-tag`, y comprueba
  los seis archivos antes de publicar. No se marca como prerelease porque los instaladores
  resuelven `releases/latest`, que ignora prereleases.
- `distribution-check.mjs` exige las notas de la versión de `Cargo.toml`, el comando de
  instalación del README para esa versión y la entrada del CHANGELOG.
- `ARQUITECTURA_Y_VISION.md` queda marcado como snapshot histórico de `aef27fc`; conteos
  actualizados (~34 500 líneas de Rust), VSIX `0.4.0` en el README.

Verificación local (Windows, `OSTRIN_REQUIRE_CC=1`, release): 2 unitarias, 6 diferenciales y
200 de integración en verde; `ostrinc --version` → `ostrinc 0.1.0`; `--check`/`--run` de
`hello.ostrin`; `ownership_primitives.ostrin` con `--leak-check` → `live_allocations=0`; un zip
empaquetado como en el workflow pasa checksum, `--version`, `hello.ostrin` y
`--locked --run --project examples/pkg_project/main_app`. `website-check`, `website-metadata
--check` y `distribution-check` pasan. La matriz WASI necesita el WASI SDK y se valida con
`workflow_dispatch` en GitHub.

## 267. Publicación de `v0.1.0` — 2026-09-24

Los cuatro pasos de publicación se ejecutaron con autorización explícita del usuario:

1. Push de `main`: CI Windows/Linux/macOS y Pages en verde.
2. `workflow_dispatch` de release (sin publicar) en verde en los tres targets. El WASI
   workflow nunca había pasado: la URL fijada usaba el tag `wasi-sdk-34.0` (el upstream es
   `wasi-sdk-34`; el SHA-256 fijado sí coincidía con el archivo real) y el empaquetado copiaba
   `ostrinc` en lugar de `ostrinc.wasm`. Tras ambas correcciones la matriz de nueve programas
   pasa en GitHub.
3. Tag anotado `v0.1.0` sobre `34ead55`; release y WASI del tag en
   verde. La GitHub Release contiene los tres archivos y sus `.sha256`, no es draft ni
   prerelease, y usa `docs/releases/v0.1.0.md` como cuerpo.
4. `install-check.yml` (nuevo, manual) instaló la release con `install.sh` en Ubuntu y macOS y
   con `install.ps1` en Windows, tanto con `latest` como con `0.1.0`, y ejecutó `--version`,
   `--check` y `--run hello.ostrin`. Instalación local limpia en Windows: checksum verificado,
   `ostrinc 0.1.0` y `hola desde Ostrin`.

Hallazgo: el comando de Windows documentado (`Invoke-WebRequest` sin `-UseBasicParsing`) se
quedó colgado más de dos minutos en Windows PowerShell 5.1 no interactivo; con
`-UseBasicParsing` descarga al instante. README y notas de la release lo usan ahora y
`distribution-check` lo exige.

## 268. Homepage 3.0 y Scientific Lab — 2026-09-24

Bloque web completo sobre la release `v0.1.0`, sin cambios de sintaxis.

**Hallazgo de compilador.** Al escribir la pestaña Units se vio que `q as km` reetiquetaba el
valor (`1500 m as km` → `1500 km`) en intérprete y nativo, por lo que las pruebas de paridad no
lo detectaban. El documento 01 §3.4 define `(a + b) as nm` como conversión. `Expr::As` convierte
ahora con `convert(v, from, to)` en el intérprete y con `ostrin_convert` en el emisor AST (HIR/IR
ya caían a ese emisor). Regresión: `examples/unit_conversion.ostrin` con salida exacta. Siguen
como limitaciones: `as` solo acepta un identificador de unidad, las unidades derivadas se
imprimen sin simplificar y `unit`/`define` no están implementados. También apareció que un
parámetro de tipo función no sombrea a una función global homónima (E1041), documentado en §6
de `ESTADO_Y_PLAN.md` y propuesto como tarea separada. `--help` lista ahora `--test`.

**Lab.** Ocho programas (`examples/lab_*.ostrin`, `plot_project/lab`, `autodiff_project/lab`) con
parámetros de la forma `nombre = número`. `website/ostrin-runtime.js` monta proyectos
multiarchivo como directorios WASI anidados en memoria, así que los paquetes reales `plot` y
`autodiff` corren en el navegador. `lab.js` sustituye parámetros conservando el tipo del literal
(`1` → `1.0` si el valor original era Float), dibuja gráficos solo a partir de líneas impresas por
Ostrin (`bin`, `estimate`, `point`) y muestra el SVG del paquete `plot` como `<img>`.

**Evidencia.** `scripts/lab-data.mjs` ejecuta con `website/ostrinc.wasm` bajo Node WASI los ocho
programas, el héroe y el ejemplo de pipeline (`--hir`, `--ir`, `--emit-c`), y escribe
`website/lab-data.js`. En modo check falla si cambia una fuente o una salida, si un
`<pre data-output-source>` muestra una línea que su programa no imprime o si la Reference lista
un flag ausente de `--help`. Esto destapó que la tarjeta autodiff del showcase mostraba una
salida parafraseada; ahora muestra líneas reales. `site-facts.mjs` deriva `releaseStatus`,
`releaseDate` y `releaseUrl` del encabezado fechado del CHANGELOG y de `docs/releases/`, y
`website-check.mjs` rechaza claims de release, comandos de instalación y enlaces al repositorio
que no coincidan.

**Verificación.** En el navegador, las ocho demos recalculadas en vivo reproducen exactamente la
salida registrada, y todos los sliders en sus extremos recalculan sin errores. La suite
Playwright (6 pruebas, ejecutada localmente con Chrome) cubre el Lab, el Cookbook, la navegación y
el desbordamiento en 390/768 px de las 12 páginas. La suite Rust: 2 unitarias, 6 diferenciales y
201 de integración.

## 269. Parámetros función que sombrean funciones globales — 2026-09-24

Encontrado al escribir el Lab de autodiff: con un `fn f` global, `fn combine(f: fn(Float, Float)
-> Float, ...)` fallaba con E1041 en `f(a, b)`, también dentro de un paquete importado
(`autodiff.gradient2(f: ...)`). El checker resolvía primero builtins y `self.functions`, y el
intérprete hacía lo mismo en `eval_call`, así que ambos llamaban a la función global. El emisor
nativo ya resolvía bien (`4`).

Ahora un binding local tiene prioridad: `check_call` comprueba `scope.contains_key(name)` antes de
builtins, constructores y funciones globales (también para inferir el tipo esperado de lambdas),
y `eval_call` usa `env.contains(name)`. Las funciones globales no viven en `Env` (el closure se
crea al evaluarlas como valor), así que ese test solo acierta con bindings locales reales. Las
llamadas a través de un valor función comprueban además el número de argumentos (E1041).

Pruebas: `function_value_shadowing.ostrin` (20, 5, 6; paridad nativa con `live_allocations=0`)
y `function_value_arity_errors.ostrin`, más una prueba de integración exacta. Suite: 2
unitarias, 6 diferenciales y 202 de integración. Las salidas del Lab no cambian.

## 270. Álgebra de unidades — 2026-09-24

Cierra las limitaciones anotadas en §268. Un único catálogo (`types::unit_info`, replicado en
`qty_runtime.c`) da dimensión y factor SI a cada símbolo; se añaden N, J, W, Pa, Hz, V, ohm, C,
bar, mmHg, cal y prefijos (`um`, `ns`, `mA`, `mL`, …). `atm` valía 1 Pa en el runtime: ahora
101 325 Pa. `unit_combine` produce unidades canónicas en `*`/`/` (agrupa exponentes y funde átomos
simples de la misma dimensión ajustando el valor); esto además corrige factores erróneos de cadenas
como `km/h*h`, que el parser izquierda→derecha leía como `(km/h)*h`. `as` acepta unidades compuestas
y comprueba la dimensión (E1026); los literales admiten `m^2` y exponentes negativos y solo absorben
símbolos de unidad conocidos tras `*`/`/`. Dimensiones con nombre (`Energy`, `Velocity`, …) se
expanden a base; los diagnósticos usan `dim_describe`. Nuevos `q.value()`/`q.unit()`.
Pruebas: `unit_algebra.ostrin` (salida exacta y paridad nativa), `unit_conversion_errors.ostrin`.

## 271. Correcciones del lenguaje encontradas con std.viz — 2026-09-24

- Intérprete: el cuerpo de una función se evaluaba en un hijo del entorno del llamador, y como
  `x = e` es `Stmt::Assign`, `h = hash(seed)` en `uid` reescribía la `h` de `render`. Ahora cada
  llamada usa un entorno raíz nuevo (`function_scope_isolation.ostrin`).
- Métodos: el checker comparaba argumentos por posición y el intérprete los ligaba por posición;
  ahora ambos usan `hir::arrange_arguments` (`method_default_args.ostrin`). Los defaults de métodos
  se tipan en su declaración y la HIR/C los baja con tipo.
- `modules.rs` reescribía cualquier identificador igual a un item del módulo: un parámetro `light`
  se convertía en `std.viz::light`. La reescritura sigue ahora los bindings léxicos.
- C: una sentencia con resultado propio se emitía dos veces (`code;` + `release(code)`), así que
  `c.add(1).add(2)` ejecutaba cada llamada dos veces; receptores y argumentos frescos de métodos se
  liberan tras la llamada; la asignación con retain/release solo se aplicaba en el nivel superior de
  la función (`scopes.len() == 2`) y dentro de bucles dejaba punteros colgantes (`idx = merged` en
  `argsort`). Ahora aplica fuera de lambdas.
- Literales científicos (`1e-9`); métodos genéricos infieren parámetros de dimensión; `[]` en un
  campo toma el tipo del campo; `String.slice/char_at/codepoint` tipados. Trinquetes HIR y de
  expresiones tipadas bajan de 26/11 a 19/8.

## 272. std.viz 0.1 y galería web — 2026-09-24

Documento 23. `compiler/std/viz.ostrin` (≈1 400 líneas de Ostrin) implementa figuras 2D, heatmaps y
contornos, escenas 3D (triángulos ordenados por profundidad con luz direccional, trayectorias con
mapa de color, nubes de puntos), `viz.grid` y ejes con unidades. Los diez `examples/viz_*.ostrin`
más `lab_plot`/`lab_surface` son idénticos byte a byte entre intérprete y nativo (prueba
diferencial). Web: `viz.html` con diez figuras grabadas por `ostrinc.wasm` en `website/assets/viz/`
(comprobadas contra deriva por `lab-data.mjs`) y botón Run live; pestañas Plot (ahora con std.viz) y
3D en el Lab; sección Visualization en la home. `lab-data.mjs` ejecuta cada programa en un proceso
Node propio: muchas instancias WASM pesadas en un solo proceso hacían caer a Node. Suite: 2
unitarias, 6 diferenciales, 210 de integración; 7 pruebas Playwright en verde (localmente con el
shim WASI servido por `page.route`). Después, `+` entre `String` libera los operandos frescos
(concatenaciones y resultados de llamadas) en los emisores AST y HIR: la galería nativa pasa de
233 855 a 18 652 asignaciones vivas al salir, con salida idéntica. Pendiente: argumentos `String`
frescos pasados a funciones y a `push`, interacción, animación, PNG/PDF y WebGPU.

## 273. Arrays de cantidades — 2026-09-24

`Array<Quantity<D>>` con una unidad común por array (modelo NumPy+pint): en el intérprete,
`ArrayData.unit`, y `interpreter/qarray.rs` aplica a los números las reglas de los escalares
(`+`/`-`/comparaciones convierten el lado derecho a la unidad del izquierdo con la fórmula de
`convert`; `*`/`/` usan `unit_combine` y su factor; un resultado sin dimensión es `Array<Float>`).
En nativo, el mismo `Array_Float` con un campo `unit` (todas las instancias de array lo tienen, NULL
para arrays simples) y los helpers `ostrin_qa_*`; `mangle_ctype(Array<Quantity>)` es `Array_Float`.
El checker valida dimensiones (E1024/E1026) y tipa reducciones (`var` eleva la unidad al cuadrado).
`std.viz` gana `unit_line`/`unit_scatter` y `viz_units.ostrin` usa arrays. Pruebas:
`quantity_arrays.ostrin` (salida exacta y paridad) y `quantity_arrays_errors.ostrin`; suite 211
integración, 6 diferenciales, 2 unitarias; Playwright 7/7.

## 274. Temporales nativos — 2026-09-24

Con un volcado temporal del registro de memoria sobre la galería Viz se localizaron las fugas: `push`
retiene su valor pero el llamador nunca soltaba un valor fresco; los receptores frescos de métodos de
`String` y de arrays no se liberaban; los operandos frescos de operaciones con arrays tampoco; y
liberar un array solo liberaba la cabecera (`shape` y `data` quedaban vivos: ahora hay `@N@_drop`).
La galería pasa de 233 855 a 303 asignaciones vivas al salir (pico 3 987) con SVG idénticos;
`native_memory_temporaries.ostrin` llega a `live_allocations=0` y tiene su prueba.

## 275. Interacción en std.viz y un use-after-free del emisor AST — 2026-09-24

std.viz 0.2 (parcial): cada figura lleva un `<style>` con resaltado al pasar el ratón y `<title>` con
los valores de puntos, barras, barras de error y puntos 3D; funciona donde se abra el SVG, sin
scripts, así que no contradice la regla de que JavaScript no calcula resultados. La galería añade
"Explore": el SVG en un `iframe sandbox` con zoom y desplazamiento. Al añadirlo, `viz_dashboard`
falló en nativo: `gen_block_expr` consideraba "transferido" un local del bloque exterior en
`if c { line } else { … }`, no lo retenía y el bloque exterior lo liberaba (AddressSanitizer). La
transferencia se limita a locales del propio bloque. Todos los ejemplos viz pasan ASan.

## 276. Unidades declaradas por el programa — 2026-09-24

`dimension`, `unit` y `define` (palabras reservadas desde el documento 17) ya funcionan. El parser
hace una pasada previa por cada archivo (dimensiones, unidades, definiciones) y las registra en un
registro por compilación de `types.rs` (`thread_local`, reiniciado en `load_project` para el LSP);
`unit_info` consulta el catálogo y después ese registro, así que el parser, el checker, el intérprete
y `resolve_unit_*` las ven sin cambios. El runtime C recibe una tabla `ostrin_user_units` generada;
las unidades simples usan el mismo código de dimensión base que las del catálogo para fundirse
(`ft * m`). Errores: dimensión desconocida, símbolo ya existente, `define` de unidad no declarada o
entre dimensiones distintas. De paso, `within` comparaba los números sin convertir unidades
(`6 ft within (1.5 m to 2 m)` daba false) en ambos backends.
