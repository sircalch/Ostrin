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

- **`Value`** (en `interpreter/mod.rs`): `Int, Float, Bool, Char, String, Quantity(f64, Dimension, unit_str), List(Rc<RefCell<Vec<Value>>>), Closure, Record(String, Rc<RefCell<Vec<(String,Value)>>>), EnumInstance(enum, variant, HashMap<String,Value>), Task, Channel, Map(Rc<RefCell<Vec<(K,V)>>>), Set(Rc<RefCell<Vec<Value>>>), Void`.
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
