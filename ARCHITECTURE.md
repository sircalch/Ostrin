# Ostrin — arquitectura

Este archivo es el índice canónico de la arquitectura actual y objetivo de Ostrin.

- [Visión y arquitectura del lenguaje](docs/ARQUITECTURA_Y_VISION.md)
- [Pipeline HIR/IR y migración del backend](docs/design/20-hir-y-ir.md)
- [Estado verificado y plan activo](ESTADO_Y_PLAN.md)
- [Contexto histórico de decisiones](CONTEXTO_PROYECTO.md)

El pipeline del compilador es:

`source → lexer → parser → AST → resolución → type checker → HIR tipado → monomorfización → Ostrin IR → ownership → backends`

El intérprete sigue siendo la referencia semántica. El backend nativo transpila a C y el backend
WASI produce el compilador WASM; ambos se validan con pruebas diferenciales y los gates de
seguridad descritos en [SECURITY.md](SECURITY.md).
