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

El runtime C generado separa ahora sus dos superficies: el modo cooperativo por defecto usa
locks no-op y no incluye headers de pthread/Windows para hilos; solo `--native-threads` define
`OSTRIN_NATIVE_THREADS` y activa mutexes, condiciones y threads del sistema operativo. Esto
reduce la dependencia del C generado para un futuro toolchain WASI, sin afirmar todavía que
ese toolchain esté configurado ni que `ostrinc --compile` produzca WASM.

## Reproducir localmente

```powershell
rustup target add wasm32-wasip1
cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release
```

El resultado queda en
`compiler/target/wasm32-wasip1/release/ostrinc.wasm`. Para usarlo hace falta un runtime
WASI 0.2 compatible; el workflow conserva el checksum para que una descarga se pueda verificar
antes de ejecutarla.

## Verificación ejecutable

El workflow también arranca ostrinc.wasm bajo Node WASI preview1 con
--check examples/hello.ostrin y un preopen del workspace. Esto verifica que el módulo
acepta argumentos, puede leer un archivo Ostrin y devuelve código de salida cero.
La misma comprobación local puede ejecutarse, después de compilar, con:

    node --input-type=module -e "import { WASI } from 'node:wasi'; import { readFileSync } from 'node:fs'; const wasi = new WASI({ version: 'preview1', args: ['ostrinc', '--check', 'examples/hello.ostrin'], preopens: { '.': process.cwd() }, returnOnExit: true }); const mod = await WebAssembly.compile(readFileSync('compiler/target/wasm32-wasip1/release/ostrinc.wasm')); const instance = await WebAssembly.instantiate(mod, wasi.getImportObject()); const code = wasi.start(instance); if (code !== 0) process.exit(code);"

## Siguiente etapa

El backend de programas Ostrin todavía emite C; no se debe afirmar que
`ostrinc --compile` produzca WASM. El siguiente bloque de esta línea es seleccionar un
toolchain C/WASI, aislar las APIs de proceso y archivos que aún usa el runtime, y añadir un
smoke test de un programa Ostrin compilado a WASM. Después se puede construir un adaptador de
navegador/playground sobre una API de compilación sin filesystem implícito.
