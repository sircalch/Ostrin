# Ostrin — Distribución y WebAssembly

## Estado actual

La primera superficie WASM distribuible es el propio compilador `ostrinc` compilado para
`wasm32-wasip1`. El workflow `.github/workflows/wasi.yml` lo construye en modo release,
lo empaqueta con su SHA-256 y publica un artefacto `ostrinc-wasm32-wasip1` en ejecuciones
manuales o al crear un tag `v*`.

Esto es un binario WASI, no un módulo para ejecutar directamente en una página web. Un host
WASI debe proporcionar la interfaz de sistema de archivos y argumentos que usa la CLI. La
superficie de LSP/DAP y el backend C no se presentan como compatibles con navegador en esta
etapa.

## Reproducir localmente

```powershell
rustup target add wasm32-wasip1
cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release
```

El resultado queda en
`compiler/target/wasm32-wasip1/release/ostrinc.wasm`. Para usarlo hace falta un runtime
WASI 0.2 compatible; el workflow conserva el checksum para que una descarga se pueda verificar
antes de ejecutarla.

## Siguiente etapa

El backend de programas Ostrin todavía emite C; no se debe afirmar que
`ostrinc --compile` produzca WASM. El siguiente bloque de esta línea es aislar un runtime C
portable sin pthreads ni APIs de proceso, seleccionar una interfaz de archivos WASI y añadir
un smoke test de un programa Ostrin compilado a WASM. Después se puede construir un adaptador
de navegador/playground sobre una API de compilación sin filesystem implícito.
