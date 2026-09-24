# Ostrin — auditoría, mapa arquitectónico y arquitectura objetivo

*Base: commit `aef27fc` (código idéntico a `90e525e`; 93 pruebas en verde; la Etapa 0 posterior añade 3 más, ver §85 de CONTEXTO). Este documento responde a las partes A–E del «Master Development Prompt». Todo lo que se afirma como existente se ha comprobado en el código; lo demás está marcado como **propuesta**.*

> **Documento histórico.** Las cifras y estados de este documento corresponden al commit
> `aef27fc` y no se actualizan. El estado vigente (≈34 500 líneas de Rust, 200 pruebas de
> integración, HIR/IR con fallback verificado, concurrencia nativa, WASI, release `0.1.0`
> preparada) está en [`ESTADO_Y_PLAN.md`](../ESTADO_Y_PLAN.md); la historia completa, en
> [`CONTEXTO_PROYECTO.md`](../CONTEXTO_PROYECTO.md).

---

## A. Auditoría del repositorio

**171 archivos versionados.** Rust ~14 500 líneas en `compiler/src/`.

| Área | Archivo(s) | Líneas | Estado real |
|---|---|---|---|
| Léxico | `lexer/` | ~410 | Completo para la sintaxis actual |
| Parser | `parser/mod.rs` | 1 055 | Completo; AST con rangos (`Expr::Located`) |
| AST | `ast.rs` | 274 | Estable; sin identificadores de nodo ni tipos anotados |
| Módulos/paquetes | `modules.rs`, `package.rs`, `ostrin.toml` | 610 | Imports y dependencias locales; lockfile básico; sin registro |
| Verificador de tipos | `typeck/mod.rs` | 3 551 | Muy completo (dimensiones, traits, genéricos, exhaustividad). **Produce errores, no un AST tipado** |
| Tipos | `types.rs` | 167 | `Ty` con `Unknown` (84 usos en el checker) y `Generic` |
| Intérprete | `interpreter/mod.rs` | 2 372 | Dinámico (`Value`), semántica de referencia; con hooks de depuración |
| Backend nativo | `codegen.rs` + `qty_runtime.c` | ~3 900 | Transpila a C; ~90 % de los ejemplos ejecutables |
| LSP / DAP | `lsp.rs`, `dap.rs`, `symbols.rs`, `protocol.rs` | ~1 700 | Funcionales sobre stdio |
| CLI | `main.rs` | 559 | `--check/--run/--emit-c/--compile/--lsp/--dap/--json…` |
| VS Code | `vscode-ostrin/` | — | v0.4.0, VSIX |
| Pruebas | `compiler/tests/examples.rs` | — | 93 pruebas de integración (sin pruebas unitarias por módulo) |
| CI | `.github/workflows/ci.yml`, `pages.yml` | — | **Existe**: `cargo test` en Ubuntu (sin Windows/macOS ni prueba del backend nativo garantizada) |
| Docs | `docs/design/` (17), `CONTEXTO_PROYECTO.md` (84 secciones) | — | Diseño abundante; parte es anterior al código |

**Implementado y sólido:** front‑end, checker, intérprete, LSP/DAP, backend nativo con monomorfización.
**Parcial:** paquetes (sin registro), concurrencia (simulada), stdlib (mínima), memoria nativa (sin liberar).
**Solo documentado:** `select`, cancelación de tareas, `Hash`, gran parte del ecosistema científico.
**No existe:** IR, optimizador, tipos numéricos de ancho fijo, arrays/matrices, autodiff, GPU, visualización, DataFrame, `fmt`/`test` como comandos, FFI.

---

## B. Mapa arquitectónico

```text
                      ┌───────────────┐
 Source ─► Lexer ─► Parser ─► AST ─┬─► Modules (resolución de imports, reescribe AST)
                                   │
                                   ├─► Checker (typeck) ──► [errores, EditorBinding/EditorExpression: solo strings]
                                   │        usa  Ty
                                   │
                                   ├─► Interpreter ──► Value (dinámico)   ◄── referencia semántica
                                   │
                                   └─► Codegen ──► CType (reinfiere tipos) ──► C ──► gcc/clang
                                            ▲
                                            └── (no consume nada del checker)
```

### Acoplamientos y duplicaciones (los importantes)

1. **Tres representaciones de tipo**: `ast::Type` (sintáctica), `types::Ty` (checker), `codegen::CType` (backend). Cada una con su propia noción de genérico, `Option`, cantidad, etc.
2. **El backend reinfiere todo.** `codegen.rs` no usa nada del checker: recalcula tipos de expresiones, inferencia de genéricos (`bind_type`), instanciación y hasta pistas de tipo esperado (`expected`). Es la fuente principal de divergencias posibles con el checker y con el intérprete (ya hubo tres: `Int/Int`, `Ok(x)` sin `E`, `Nothing` sin `T`).
3. **El checker no entrega un AST tipado.** Hoy solo exporta `type_name: String` por expresión al editor. No hay `NodeId`, ni tabla de tipos por nodo, ni resolución de nombres materializada.
4. **`Ty::Unknown` (84 usos)**: el checker es deliberadamente permisivo; un IR tipado exigirá cerrar esos huecos o representarlos explícitamente.
5. **Intérprete dinámico**: comprueba en ejecución cosas que el checker no fija (p. ej. E1101 «movido tras enviar»); por eso el nativo no puede replicarlas sin análisis estático.
6. **Módulos reescriben el AST** (`modules.rs`) antes de todo: cualquier IR debe nacer *después* de esa resolución.
7. **`codegen.rs` es un archivo de ~3 900 líneas** con responsabilidades mezcladas (tipos, monomorfización, generación de expresiones, runtime de listas/mapas/canales en cadenas de texto C).
8. **Pruebas solo de extremo a extremo**: cubren mucho, pero un fallo no se localiza por fase.

### Riesgos
- Cada capacidad nueva (arrays, unidades más ricas, autodiff) multiplicaría la reinferencia por tres.
- Sin modelo de memoria ni IR, la optimización (SIMD, paralelismo, GPU) no tiene dónde apoyarse.
- Semántica dinámica del intérprete vs estática deseada: hay que decidir cuál es *la* especificación.

### Qué se conserva
Lexer, parser, AST (con extensiones), módulos, mensajes/códigos de error, LSP/DAP, intérprete como **oráculo de pruebas**, el runtime de cantidades (`qty_runtime.c`) como prototipo del enfoque «dimensión estática, unidad dinámica», y toda la suite de ejemplos.

### Dónde entra el IR
Entre el checker y los backends: **el checker deja de ser solo un validador y pasa a producir HIR tipado** (§D). El intérprete puede seguir sobre AST durante la migración y, más adelante, pasar a ejecutar el IR (una sola semántica).

---

## C. Matriz competitiva (estado honesto)

Leyenda: ● sólido · ◐ parcial · ○ ausente. «Ostrin» = hoy; «Meta» = qué puede diferenciarlo.

| Eje | Python | Julia | Rust | MATLAB | R | **Ostrin hoy** | Meta diferenciadora |
|---|---|---|---|---|---|---|---|
| Núcleo del lenguaje | ● | ● | ● | ◐ | ◐ | ◐ (records, enums, traits, genéricos, match) | Coherencia y estabilidad desde 0.x |
| Sistema de tipos | ◐ (opcional) | ● | ● | ○ | ○ | ● checker estático con genéricos y traits | Tipos científicos nativos |
| Unidades/dimensiones | ○ (libs) | ◐ (Unitful) | ○ (libs) | ◐ | ○ | **● integrado en el tipo** | Ya es diferenciador; extender a incertidumbre |
| Numérica (anchos, complejos…) | ● | ● | ● | ● | ● | ○ (solo `Int`/`Float`) | Jerarquía numérica explícita y segura |
| Arrays/matrices/tensores | ● (NumPy) | ● | ◐ (crates) | ● | ● | ○ (solo `List`) | **Pilar nº 1 por construir** |
| Estadística | ● | ● | ◐ | ● | ● | ○ (`sum`) | stdlib propia coherente |
| Visualización | ● | ● | ◐ | ● | ● | ○ | API declarativa propia |
| Concurrencia | ◐ (GIL) | ● | ● | ◐ | ○ | ◐ simulada síncrona | Segura por construcción (E1100 ya existe) |
| GPU | ● (libs) | ● | ◐ | ● | ◐ | ○ | Modelo propio sobre IR |
| ML / autodiff | ● | ● | ◐ | ● | ◐ | ○ | Autodiff en compilador (IR) |
| Paquetes | ◐ | ● | ● | ◐ | ◐ | ◐ local + lockfile | Reproducible, red explícita |
| Reproducibilidad | ◐ | ◐ | ◐ | ○ | ◐ | ○ | `experiment` con metadatos (diseño nuevo) |
| IDE | ● | ◐ | ● | ● | ◐ | ● LSP + semantic tokens | Mantener; añadir acciones de código |
| Depuración | ● | ◐ | ● | ● | ◐ | ● DAP real (intérprete) | Extender al nativo |
| Rendimiento | ○ | ● | ● | ◐ | ○ | ◐ nativo vía C, sin optimizador ni liberar memoria | IR + optimizador |
| Despliegue | ◐ | ◐ | ● | ○ | ○ | ◐ ejecutable vía gcc; sin instalador | Binario único + WASM |

**Ya hace bien:** dimensiones en el tipo, checker con exhaustividad y traits, herramientas de editor, semántica dual comprobada (intérprete/nativo).
**Le falta:** todo el eje numérico‑científico (arrays, tipos numéricos, estadística, plots, DataFrame), memoria, concurrencia real, optimizador.
**Puede hacer mejor:** unidades + incertidumbre + reproducibilidad como ciudadanos de primera.
**Debe diseñarse desde cero:** modelo de arrays/layout, modelo de memoria, IR, modelo de paralelismo/GPU, formato de experimento.

*Nota de honestidad:* superar a Python/Julia en ecosistema es un objetivo a años; el plan compite en ejes concretos (unidades, seguridad, reproducibilidad, diagnósticos, despliegue), no en número de paquetes.

---

## D. Arquitectura objetivo (1.0+)

```text
Source
 ↓ Lexer → Parser → AST (con NodeId)
 ↓ Module/Name Resolution            (símbolos únicos, imports resueltos)
 ↓ Type System                       (una sola representación de tipos: `Ty`, sin Unknown)
 ↓ Typed AST / HIR                   (cada nodo con tipo; genéricos aún abstractos)
 ↓ Monomorphization                  (HIR concreto)
 ↓ Ostrin IR (MIR)                   (SSA/CFG, propiedad/movimientos explícitos, unidades ya borradas o verificadas)
 ↓ Optimizer                         (inlining, folding, DCE, bucles, vectorización)
 ├─► C backend        (actual, para mantener mientras sirva)
 ├─► LLVM / nativo    (objetivo de rendimiento)
 ├─► WASM
 └─► GPU (SPIR-V/PTX vía IR de kernels)
Runtime: memoria (ownership/arenas/RC), hilos/tareas/canales, arrays, unidades
FFI: C primero; luego C++/Fortran/Python/Julia
Stdlib (core) + Scientific library (arrays, álgebra lineal, estadística, autodiff, plot, dataframe)
Herramientas: CLI `ostrin` (new/build/run/test/fmt/check/bench/doc/add), LSP, DAP, gestor de paquetes
```

Principios de migración (todos incrementales, sin reescrituras):
- Cada etapa nueva convive con la anterior detrás de una bandera hasta igualar resultados con la suite y con el oráculo (intérprete).
- El intérprete se conserva como oráculo hasta que exista la ejecución de IR.

---

## E. Ruta incremental desde `aef27fc`

**Decisión que lo desbloquea todo:** que el checker produzca un **HIR tipado** y que el backend nativo lo consuma en lugar de reinferir.

### Etapa 0 — Red de seguridad (antes de tocar arquitectura)
1. Pruebas **diferenciales automáticas** intérprete↔nativo (todos los `examples/`), como test único parametrizado; hoy están repartidas.
2. CI ampliado: Windows + Linux + macOS con compilador C obligatorio (**hecho**, §85 de CONTEXTO).
3. Harness de *compile‑fail* (ya existen ejemplos `*_errors.ostrin`; sistematizar con el código de error esperado).
4. Fuzzing básico del lexer/parser (no debe hacer panic).

### Etapa 1 — Tipos anotados
1. Añadir `NodeId` al AST (identificador estable por expresión/patrón/declaración).
2. Que `typeck` registre `NodeId → Ty` (tabla) y resolución de nombres; primero como datos paralelos, sin cambiar su comportamiento.
3. Reducir `Ty::Unknown`: cada uso pasa a error explícito, tipo `Never`/inferencia pendiente, o se documenta como permisivo.

### Etapa 2 — HIR y backend sobre HIR
1. Definir HIR (árbol tipado, nombres resueltos, sin azúcar: `for`/`try`/patrones desazucarados).
2. Portar el codegen para consumir HIR: se eliminan `bind_type`, `expected`, `settle_literal`, etc. (la inferencia vive en un solo sitio).
3. Validar con la suite diferencial tras cada porción (funciones → records → enums → genéricos → …).

### Etapa 3 — Modelo de memoria (decisión de diseño explícita)
Especificar (documento nuevo) ownership/borrowing simplificado vs. arenas vs. RC; implementar el elegido en el nativo. Esto desbloquea funciones como valores (cierres con entorno) y E1101 estático.

### Etapa 4 — IR y optimizador mínimo
Ostrin IR (CFG), monomorfización sobre IR, inlining y plegado de constantes; el backend C pasa a generar desde IR.

### Etapa 5 — Núcleo científico
Jerarquía numérica; `Array<T, N…>` con layout y vistas; broadcasting; `@`; álgebra lineal; después estadística, autodiff sobre IR, `parallel for`.

Cada etapa sigue la regla del prompt: semántica → ejemplos → tests → implementación → intérprete → nativo → comparar → documentar.

---

## Decisiones abiertas (necesito tu criterio)

1. **Especificación semántica**: ¿el intérprete sigue siendo la referencia, o pasa a serlo un documento de especificación + IR (recomendado a medio plazo)?
2. **Modelo de memoria**: ¿ownership/borrowing (más seguro, más complejo), arenas + RC (más simple)? Recomiendo decidir tras prototipar ambos sobre funciones‑valor y E1101.
3. **Backend de rendimiento**: mantener C mientras se construye IR y evaluar LLVM (dependencia grande) frente a un backend propio pequeño.
4. **Alcance inmediato**: ¿empezamos por Etapa 0+1 (fundamentos, sin cambios visibles) o por una porción científica visible (tipos numéricos + arrays) sobre la arquitectura actual, aceptando reinferencia temporal?

Mi recomendación: **Etapa 0 y 1 primero** (1–2 semanas de trabajo), porque cada feature científica que se añada antes se paga tres veces.
