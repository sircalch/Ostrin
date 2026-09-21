# 22. Biblioteca estándar (`std`)

La biblioteca estándar está escrita en Ostrin, embebida en el compilador (`compiler/std/*.ostrin`)
y se importa como cualquier módulo: `import std.math`, `import std.lists`, `import std.strings`,
`import std.time`.
Compila por los dos backends (intérprete y nativo) sin código especial.

| Módulo | Funciones |
|---|---|
| `std.math` | `min`, `max`, `clamp` (genéricas, `T: Ord`), `sign`, `gcd`, `lcm`, `pow_int` |
| `std.lists` | `contains`, `index_of`, `reversed`, `take`, `drop_first`, `concat`, `repeat`, `range_list`, `max_of`, `min_of`, `sorted` (estable) |
| `std.strings` | `repeat`, `pad_left`, `pad_right`, `count_of`, `join_with` |
| `std.time` | `Date`, `date`, `is_leap_year`, `days_in_year`, `days_in_month`, `is_valid`, `day_of_year`, `day_of_week`, `from_day_of_year`, `iso`, `parse_iso` |

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
- **Un módulo `std` local o una dependencia llamada `std` tiene prioridad** sobre la biblioteca
  embebida (el `ostrin.toml` del proyecto manda).
- Las pruebas de la propia biblioteca están en Ostrin (`examples/std_tests.ostrin`, `--test`).

Limitaciones conocidas: una lista vacía literal no permite inferir `T` (`lists.sorted([])` exige
una variable tipada); el ordenamiento es O(n²) por inserción.
