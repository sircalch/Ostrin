# Ostrin — Distribución y WebAssembly

## Estado actual

La superficie WASM distribuible incluye el propio compilador `ostrinc`, un programa Ostrin
independiente y un proyecto con dependencia local `path`, todos compilados para `wasm32-wasip1`. El workflow
`.github/workflows/wasi.yml` instala una versión fijada de `wasi-sdk`, compila los dos
artefactos, los ejecuta bajo Node WASI, conserva sus SHA-256 y publica un artefacto
`ostrinc-wasm32-wasip1` en ejecuciones manuales o al crear un tag `v*`.

El compilador WASI también se adapta a una página web mediante `website/playground.js`: el host
usa `@bjorn3/browser_wasi_shim`, un directorio preabierto en memoria y la misma CLI para ejecutar
`--run`, `--check`, `--test` y `--fmt` sin subir el programa. Un host WASI externo debe seguir
proporcionando la interfaz de sistema de archivos y argumentos que usa la CLI. La superficie de
LSP/DAP y el backend C no se presentan como compatibles con navegador en esta etapa.

El runtime C generado separa ahora sus dos superficies: el modo cooperativo por defecto usa
locks no-op y no incluye headers de pthread/Windows para hilos; solo `--native-threads` define
`OSTRIN_NATIVE_THREADS` y activa mutexes, condiciones y threads del sistema operativo.
Esto permite que el mismo backend C produzca un módulo WASI cooperativo; `--native-threads`
continúa siendo incompatible con ese target.

## Reproducir localmente

```powershell
rustup target add wasm32-wasip1
cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release

# Con wasi-sdk instalado y sus rutas exportadas:
cargo run --manifest-path compiler/Cargo.toml -- `
  --compile --target wasm32-wasi --out hello.wasm examples/hello.ostrin
```

El resultado queda en
`compiler/target/wasm32-wasip1/release/ostrinc.wasm`. Para usarlo hace falta un runtime
WASI 0.2 compatible; el workflow conserva el checksum para que una descarga se pueda verificar
antes de ejecutarla.

## Verificación ejecutable

El workflow arranca `ostrinc.wasm` bajo Node WASI preview1 con
`--check examples/hello.ostrin` y un preopen del workspace. Después arranca `hello.wasm`
con el mismo host. También ejecuta `pkg_project.wasm`, cuya entrada se selecciona desde
`ostrin.toml` y que importa `shared_lib` mediante una dependencia `path`. Esto verifica que
la distribución acepta argumentos, puede leer un archivo Ostrin, resuelve paquetes locales y
que el backend de programas produce comandos WASI ejecutables.
La misma comprobación local puede ejecutarse, después de compilar, con:

    node --input-type=module -e "import { WASI } from 'node:wasi'; import { readFileSync } from 'node:fs'; const wasi = new WASI({ version: 'preview1', args: ['ostrinc', '--check', 'examples/hello.ostrin'], preopens: { '.': process.cwd() }, returnOnExit: true }); const mod = await WebAssembly.compile(readFileSync('compiler/target/wasm32-wasip1/release/ostrinc.wasm')); const instance = await WebAssembly.instantiate(mod, wasi.getImportObject()); const code = wasi.start(instance); if (code !== 0) process.exit(code);"

## Playground en el navegador

`website/playground.html` ejecuta el `ostrinc.wasm` real en la página con el intérprete
(`--run`), más `--check`, `--test` y `--fmt`. Usa `@bjorn3/browser_wasi_shim` (desde jsDelivr)
como capa WASI y un directorio en memoria con un único archivo `main.ostrin`; no hay red ni
sistema de archivos del usuario. `pages.yml` compila `ostrinc.wasm` (`wasm32-wasip1`, release)
y lo copia a `website/` antes de desplegar (el binario no se versiona). Para probarlo en local:

    cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release
    cp compiler/target/wasm32-wasip1/release/ostrinc.wasm website/
    python -m http.server --directory website 8765

La compilación nativa (C) no está disponible en el navegador: no hay toolchain C ahí.

## Siguiente etapa

La ruta WASI ya existe para el runtime cooperativo básico, paquetes locales y el playground del
sitio. El siguiente bloque es ampliar la matriz de programas (I/O), aislar APIs de proceso/
archivos con contratos WASI explícitos y mejorar la experiencia del playground (compartir código,
diagnósticos y ejemplos) sin convertirlo en una simulación JavaScript.
