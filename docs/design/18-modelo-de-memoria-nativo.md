# 18. Modelo de memoria del backend nativo

*Estado: runtime con registro de allocations, callbacks de destrucción tipados para records,
colecciones y entornos de tareas, primitivas `clone`/`drop`, primer lowering conservador de
ownership y ABI `retain`/`release`. Las familias `String`, `Result` con payload escalar y
error `String`, `List<T>` escalar, wrappers `Option`/`Result` sobre listas, mapas, conjuntos y
otros wrappers, `Option<Map<Int,String>>`/`Result<Set<Int>,String>`, los núcleos escalares de `Map<K,V>`/`Set<T>`,
records concretos y `Option<T>` escalar/`Option<String>`/`Option<Record>`
ya consumen ownership desde la IR y terminan sin allocations vivas en las pruebas nativas.
Los `Phi` simples transfieren la referencia entrante sin retenerla de nuevo,
y los `Phi` de bucle liberan el valor corriente después de su último uso seguro en el backedge;
la inserción automática de RC por último uso sobre otros payloads gestionados de `Option` y
agregados complejos todavía no está completa.*

## 1. Punto de partida (semántica ya fijada por el lenguaje)

Estas propiedades vienen del intérprete y de los documentos 09 y 11, y **no** deben cambiar:

1. Los `record` tienen **identidad por referencia**: dos variables pueden apuntar al mismo record y ver los cambios de `mut` del otro.
2. `List`, `Map`, `Set` también son por referencia.
3. Los `enum`, `Option`, `Result`, cantidades y escalares son **valores**.
4. No hay sintaxis de préstamo (`&`, `&mut`) ni de movimiento: el usuario no escribe tiempos de vida.
5. Los cierres capturan por valor lo inmutable; los `mut` no se capturan en `spawn` (E1100).
6. Un record con estado mutable enviado por un canal no puede reutilizarse (E1101); los
   records inmutables se pueden compartir.

## 2. Requisitos

Sin fugas en programas de larga duración; sin doble liberación ni uso tras liberar; destrucción determinista; coste bajo y previsible; que funcione con estructuras científicas grandes (arrays); que no obligue a cambiar la sintaxis del lenguaje.

## 3. Opciones evaluadas

| Opción | Encaja con (1)–(6) | Seguridad | Coste | Complejidad en el compilador |
|---|---|---|---|---|
| **A. No liberar (actual)** | Sí | Trivial | La memoria crece sin límite | Ninguna |
| **B. Ownership + borrowing (estilo Rust)** | **No**: obliga a sintaxis de préstamo y prohíbe el aliasing libre de records | Máxima | Nulo en ejecución | Muy alta; cambia el lenguaje |
| **C. Recolector de basura (trazado)** | Sí | Alta | Pausas, runtime pesado, mal encaje con GPU/WASM | Media (runtime), baja (compilador) |
| **D. Conteo de referencias (RC)** | **Sí** | Alta salvo ciclos | Incrementos/decrementos; sin pausas | Media: exige un IR con propiedad explícita |
| **E. Arenas por ámbito** | Parcial: solo para datos que no escapan | Alta | Mínimo | Media: requiere análisis de escape |

## 4. Recomendación

**RC determinista (D) como base, más arenas (E) como optimización, y ownership solo como análisis interno.**

La primera etapa ya implementada centraliza las reservas en `ostrin_alloc`, `ostrin_calloc`,
`ostrin_realloc` y `ostrin_free`, registra los bloques para liberarlos al salir y expone
`ostrin_retain`/`ostrin_release` como ABI del futuro lowering. Los objetos compuestos registran
además un callback de destrucción que libera sus buffers y referencias hijas. Los entornos
capturados por tareas tienen un destructor separado que se ejecuta al terminar normalmente,
al cancelar en un checkpoint o al descartar una tarea pendiente; `clone`/`drop`
permiten ejercitar el contrato de forma explícita. `--leak-check` imprime las asignaciones
vivas, el pico y el total antes de la limpieza. Esto resuelve la destrucción tipada de los
casos explícitos, y las primeras familias IR de `String`/records/`List<T>`/`Map`/`Set`/`Option` ya
insertan/consumen `retain/release` en retornos, aliases, `phi`, `print` y elementos retenidos
por los helpers de colecciones. Los `Phi` simples transfieren la referencia entrante sin
retenerla de nuevo; los `Phi` de bucle liberan el valor corriente tras su último uso seguro en
el backedge. `Option<T>` con payload escalar es un struct C por valor y no
requiere RC; `Option<String>` retiene/libera condicionalmente su puntero y ya cubre
`Some`/`None` simples; las listas con elementos escalares o records pueden vivir dentro de
`Option<List<T>>` y `Result<List<T>,E>` con el mismo retain/release condicional del puntero.
`Option<Map<Int,String>>` y `Result<Set<Int>,String>` hacen lo propio sobre los callbacks de
destrucción de sus contenedores. Los records concretos pueden anidarse y vivir dentro de
`Option` con el mismo contrato.
`Result<Int,String>` y `Result<Float,String>` también retienen/liberan
condicionalmente el campo activo y cubren los consumidores básicos de `String.to_int()` y
`to_float()`, además de `try` y `try catch` inline, desde la IR. Los wrappers anidados cubren
constructores y `match` con retain/release recursivo; `unwrap` anidado, patrones más profundos,
escapes complejos y handlers locales/closures no inline todavía no insertan RC completo por
cada copia, retorno, phi o salida de ámbito. Un alias local de una función global sin entorno
(`handler = recover`) sí conserva su procedencia estática y baja a una llamada directa; el
valor `Fn` auxiliar se trata como puntero estático sin RC. Las llamadas indirectas y closures
con entorno siguen pendientes.

- Todo valor por referencia lleva un contador. `retain`/`release` los inserta el compilador **sobre el IR** (no sobre el texto C), en copias de variable, paso a funciones, campos y salida de ámbito.
- **Análisis de último uso / movimiento** en el IR: si el compilador prueba que un valor no se vuelve a usar, transfiere la propiedad sin tocar el contador (así se recupera el coste cero en el caso común, sin sintaxis nueva).
- **Ciclos**: son la debilidad conocida del RC. Decisión: (1) documentarlo, (2) proporcionar `Weak<T>` en la biblioteca estándar para grafos y padres, (3) un modo de depuración `--leak-check` que informe de lo no liberado al salir.
- **E1101 (valor movible enviado por canal)** ya es un **análisis estático** de movimiento sobre
  el mismo IR en las rutas normales: enviar por un canal *mueve* el valor; usarlo después es un
  error de compilación. Un análisis de flujo sobre el CFG alcanzable converge en loops, combina
  conservadoramente los caminos que se juntan y comprueba los operandos `Phi` en su arista de
  entrada seleccionada. La clasificación distingue records/enums con estado mutable, mientras que
  records inmutables se pueden compartir; las guardas dinámicas quedan como red de seguridad y
  el estado de movimiento vive exactamente tanto como el allocation gestionado para que una
  dirección reciclada no herede el movimiento de un objeto destruido.
- **Arrays científicos** (documento 19) usan buffers con propietario único y vistas prestadas: es el único sitio donde sí hay préstamos, y son internos a la biblioteca.
- **Concurrencia real**: contadores atómicos solo en valores que cruzan hilos (marcados por el análisis de canales y `spawn`); el resto usa contadores simples.

## 5. Por qué no antes

El RC sobre el generador actual (texto C con expresiones‑sentencia) exigiría insertar `retain`/`release` en temporales que no tienen nombre. Con un IR en forma de bloques básicos (Etapa 4 del plan) cada temporal es explícito y el análisis de movimiento es directo. Intentarlo ahora produciría un parche frágil.

## 6. Plan de implementación

1. HIR → IR con valores temporales explícitos (Etapas 2–4 de `docs/ARQUITECTURA_Y_VISION.md`).
2. Runtime C: registro de objeto con contador; `ostrin_retain`/`ostrin_release`; destructores por tipo.
   Esta base ya cubre records, listas, mapas, sets, canales y entornos de tareas generados.
3. Inserción de retain/release + optimización de último uso. Ya existe una primera pasada
   (`--ownership-ir`) que marca transferencias lineales conocidas y el runtime ofrece el ABI;
   el backend C ya consume esa IR transformada para `String`, el núcleo escalar de `List<T>`,
   las operaciones escalares de `Map`/`Set`, records concretos y `Option` escalar/`Option<String>`/
   `Option<Record>`, `Option/List`, `Result/List`, `Option/Map`, `Result/Set` y wrappers anidados con `match`, así como `Result` escalar con error `String`, `try`, `try catch` inline,
   `map`/`map_err`/`then`, `Option.map`/`then` con lambdas inline, handlers globales y aliases
   locales de handlers globales sin entorno; aún falta extenderla a otros payloads gestionados,
   llamadas indirectas, handlers locales/closures con entorno, patrones anidados, scopes, escapes complejos y payloads todavía no cubiertos por
   el análisis de `Phi`.
4. `--leak-check` y pruebas: los programas que usan ownership explícito deben terminar con cero
   objetos vivos; convertir ese objetivo en automático requiere el lowering de último uso.
5. E1101 estático integrado; mantener la comprobación dinámica del intérprete y nativo como red
   de seguridad hasta que el backend consuma completamente la IR transformada.
6. Arenas para datos que no escapan (optimización).

## 7. Preguntas abiertas

- Los `enum` que contienen records copian el puntero y hacen `retain` al copiar (lo resuelve el RC).
- ¿`Weak<T>` como tipo del lenguaje o de la biblioteca? Recomendado: biblioteca.
- ¿Modo sin RC (arena única) para scripts de vida corta? Recomendado como bandera `--arena`.
