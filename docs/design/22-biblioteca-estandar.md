# 22. Biblioteca estándar (`std`)

La biblioteca estándar está escrita en Ostrin, embebida en el compilador (`compiler/std/*.ostrin`)
y se importa como cualquier módulo: `import std.math`, `import std.lists`, `import std.strings`,
`import std.time`, `import std.json`.
También incluye `import std.args` y `import std.env` para aislar la superficie de proceso y
filesystem que ya comparten el intérprete y el backend nativo, y `import std.maps` para consultas
genéricas sobre mapas.
Compila por los dos backends (intérprete y nativo) sin código especial.

| Módulo | Funciones |
|---|---|
| `std.math` | `min`, `max`, `clamp` (genéricas, `T: Ord`), `sign`, `gcd`, `lcm`, `pow_int` |
| `std.lists` | `contains`, `index_of`, `reversed`, `take`, `drop_first`, `concat`, `repeat`, `range_list`, `max_of`, `min_of`, `sorted` (estable) |
| `std.strings` | `repeat`, `pad_left`, `pad_right`, `count_of`, `join_with`, `trim`, `split`, `lines`, `is_blank`, `format_text`, `format_float`, `char_at`, `slice`, `codepoint` |
| `std.time` | `Date`, `date`, `is_leap_year`, `days_in_year`, `days_in_month`, `is_valid`, `day_of_year`, `day_of_week`, `from_day_of_year`, `iso`, `parse_iso` |
| `std.json` | `Kind`, `Value`, `null_value`, `bool_value`, `number_value`, `text_value`, `array_value`, `object_value`, `kind`, `as_bool`, `as_number`, `as_text`, `array_items`, `object_keys`, `object_get`, `parse`, `stringify` |
| `std.args` | `all`, `count`, `at` (argumentos del programa; `--` separa opciones de `--run`) |
| `std.env` | `get`, `current_dir`, `join`, `exists` |
| `std.maps` | `count`, `is_empty`, `contains_key`, `get_or`, `keys`, `values` (`K: Hash + Eq`) |

Decisiones:

- **Funciones libres, no métodos.** El lenguaje aún no permite añadir métodos a tipos incorporados
  desde Ostrin; `lists.sorted(xs)` devuelve una lista nueva y no muta.
- **Los escalares satisfacen `Eq`, `Ord`, `Add`, `Sub`, `Mul`, `Div`, `Hash` y `Printable`** (el
  verificador los trata como implementados), de modo que `fn max<T: Ord>(a: T, b: T)` sirve para
  `Int`, `Float`, `String`…
- **Errores claros.** `import std.nope` lista los módulos disponibles.
- **Fechas deterministas.** `std.time` usa el calendario gregoriano proléptico y no consulta el reloj
  ni la zona horaria del sistema; `Date` se valida antes de calcular ordinales, día de semana o ISO.
- **Parseo explícito.** `parse_iso` acepta exactamente `YYYY-MM-DD` y devuelve `Result<Date, String>`;
  no intenta adivinar formatos locales ni convertir zonas horarias.
- **JSON determinista y portable.** `std.json` implementa el DOM, parseo estricto y serialización
  estable en Ostrin; rechaza claves duplicadas, números inválidos y escapes Unicode mal formados.
  Los pares sustitutos UTF-16 se convierten a UTF-8. `\u0000` se rechaza porque el `String` actual
  no representa NUL embebido.
- **Proceso explícito y delgado.** `std.args` sólo expone los argumentos del programa y la CLI
  del intérprete los recibe después de `--`; `std.env` delega en `env`, `cwd`, `path_join` y
  `file_exists`, sin inventar una semántica distinta por backend.
- **Mapas sin mutación implícita.** `std.maps` compone los métodos incorporados de `Map<K,V>`
  sin añadir una segunda representación: `get_or` devuelve un fallback para claves ausentes,
  `keys`/`values` devuelven listas nuevas y todas las funciones sólo exigen `Hash + Eq` en la
  clave. La mutación sigue siendo explícita mediante `mut` y `.set`/`.remove`.
- **Formato numérico explícito.** `std.strings.format_float(value, digits)` devuelve un
  `Result<String, String>` con notación decimal fija y acepta entre 0 y 18 cifras; una precisión
  inválida es un `Err`, no una salida silenciosamente truncada. La misma operación está disponible
  en los dos backends mediante el builtin interno `format_float_value`.
- **E/S y cancelación separadas.** `read_file` y `write_file` siguen siendo builtins de proceso,
  no wrappers de `std.strings`. En `--native-threads`, una tarea espera su operación mediante un
  worker nativo y un checkpoint temporizado, de modo que `Task.cancel()` puede liberar la tarea
  mientras el worker termina y limpia la llamada libc; el runtime cooperativo/WASI mantiene la
  operación síncrona. La cancelación no intenta abortar una llamada libc en curso ni inventa una
  política para red o registro remoto.
- **Un módulo `std` local o una dependencia llamada `std` tiene prioridad** sobre la biblioteca
  embebida (el `ostrin.toml` del proyecto manda).
- Las pruebas de la propia biblioteca están en Ostrin (`examples/std_tests.ostrin`, `--test`).

Limitaciones conocidas: una lista vacía literal no permite inferir `T` (`lists.sorted([])` exige
una variable tipada); el ordenamiento es O(n²) por inserción.
