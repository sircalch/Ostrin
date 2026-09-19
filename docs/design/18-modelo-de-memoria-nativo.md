# 18. Modelo de memoria del backend nativo

*Estado: base de runtime implementada; el primer lowering conservador de ownership ya existe,
pero RC/último uso completo aún no están implementados. El backend usa un registro de
allocations y cleanup global al terminar, como etapa previa a la propiedad determinista.*

## 1. Punto de partida (semántica ya fijada por el lenguaje)

Estas propiedades vienen del intérprete y de los documentos 09 y 11, y **no** deben cambiar:

1. Los `record` tienen **identidad por referencia**: dos variables pueden apuntar al mismo record y ver los cambios de `mut` del otro.
2. `List`, `Map`, `Set` también son por referencia.
3. Los `enum`, `Option`, `Result`, cantidades y escalares son **valores**.
4. No hay sintaxis de préstamo (`&`, `&mut`) ni de movimiento: el usuario no escribe tiempos de vida.
5. Los cierres capturan por valor lo inmutable; los `mut` no se capturan en `spawn` (E1100).
6. Un record enviado por un canal no puede reutilizarse (E1101, hoy comprobado en ejecución).

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
`ostrin_realloc` y `ostrin_free`, y registra los bloques para liberarlos al salir. Esto
resuelve la fuga global de los programas cortos y da una API única; no sustituye los pasos
de RC que siguen.

- Todo valor por referencia lleva un contador. `retain`/`release` los inserta el compilador **sobre el IR** (no sobre el texto C), en copias de variable, paso a funciones, campos y salida de ámbito.
- **Análisis de último uso / movimiento** en el IR: si el compilador prueba que un valor no se vuelve a usar, transfiere la propiedad sin tocar el contador (así se recupera el coste cero en el caso común, sin sintaxis nueva).
- **Ciclos**: son la debilidad conocida del RC. Decisión: (1) documentarlo, (2) proporcionar `Weak<T>` en la biblioteca estándar para grafos y padres, (3) un modo de depuración `--leak-check` que informe de lo no liberado al salir.
- **E1101 (record enviado por canal)** pasa a ser un **análisis estático** de movimiento sobre el mismo IR: enviar por un canal *mueve* el record; usarlo después es un error de compilación. Esto cierra la última brecha de semántica entre intérprete y nativo.
- **Arrays científicos** (documento 19) usan buffers con propietario único y vistas prestadas: es el único sitio donde sí hay préstamos, y son internos a la biblioteca.
- **Concurrencia real**: contadores atómicos solo en valores que cruzan hilos (marcados por el análisis de canales y `spawn`); el resto usa contadores simples.

## 5. Por qué no antes

El RC sobre el generador actual (texto C con expresiones‑sentencia) exigiría insertar `retain`/`release` en temporales que no tienen nombre. Con un IR en forma de bloques básicos (Etapa 4 del plan) cada temporal es explícito y el análisis de movimiento es directo. Intentarlo ahora produciría un parche frágil.

## 6. Plan de implementación

1. HIR → IR con valores temporales explícitos (Etapas 2–4 de `docs/ARQUITECTURA_Y_VISION.md`).
2. Runtime C: cabecera de objeto con contador; `ostrin_retain`/`ostrin_release`; destructores por tipo.
3. Inserción de retain/release + optimización de último uso. Ya existe una primera pasada
   (`--ownership-ir`) que solo marca transferencias lineales conocidas; no toca el backend C.
4. `--leak-check` y pruebas: cada ejemplo debe terminar con cero objetos vivos.
5. E1101 estático; retirar la comprobación dinámica del intérprete o mantenerla como red.
6. Arenas para datos que no escapan (optimización).

## 7. Preguntas abiertas

- Los `enum` que contienen records copian el puntero y hacen `retain` al copiar (lo resuelve el RC).
- ¿`Weak<T>` como tipo del lenguaje o de la biblioteca? Recomendado: biblioteca.
- ¿Modo sin RC (arena única) para scripts de vida corta? Recomendado como bandera `--arena`.
