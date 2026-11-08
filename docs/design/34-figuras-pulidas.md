# 34 · Figuras pulidas (`std.viz`, `Figure.polished`)

Estado: **implementado** en `compiler/std/viz.ostrin`. `Figure.polished` activa un estilo opcional sobre
las figuras de `std.viz`; las figuras que no lo llaman se renderizan igual que antes. Este documento describe
lo que el código hace hoy y lo verificado en las pruebas; no describe comportamiento futuro.

## 1. Alcance y decisión

El estilo pulido añade un acabado visual (rejilla suave, marco, relleno bajo la curva, trazo con gradiente,
marcadores con halo y un pie de figura) sin cambiar los datos ni la geometría de los ejes. Se activa de forma
explícita con una llamada encadenada, de modo que ninguna figura existente cambia de salida.

La decisión es que el pulido vive dentro del mismo módulo de figuras y produce el mismo tipo de salida: un único
`<svg>` autónomo. No añade dependencias de JavaScript ni de CSS externo. La animación, si se pide, usa SMIL, igual
que el resto del módulo.

## 2. Llamada

```
fn polished(mut self, caption: String = "", label_size: Int = 12, animate: Bool = false) -> Figure
```

- Pone `polish = true`, guarda `caption` y `label_size`, y guarda `animate` en el campo `animated`.
- `caption` es el texto de pie de figura. Vacío por defecto: no se dibuja nada.
- `label_size` es el tamaño en píxeles de las etiquetas de `note`, que en modo pulido van en negrita y centradas.
  En el estilo por defecto las notas usan tamaño 11 y alineación al inicio.
- `animate: true` añade movimiento SMIL (§4). Con `false` el SVG es estático.

## 3. Qué dibuja el estilo

Todo lo siguiente depende de `fig.polish`; sin la llamada, el código de dibujo es el de siempre.

- **Rejilla suave.** Las líneas de rejilla usan `stroke-dasharray="2 4"` y `stroke-opacity="0.8"`. Solo se dibujan
  si `gridlines` está activa y la figura no tiene una rejilla propia (`has_grid`).
- **Marco.** Con ejes activos, el marco es un `<rect>` sin relleno, con el color `theme.axis` y trazo de 1 px, en
  lugar de los dos segmentos de ejes del estilo por defecto.
- **Línea.** Cada serie `line` se dibuja como (a) un área con relleno del color de la serie y `fill-opacity="0.12"`,
  cerrada hasta la línea del cero, y (b) un trazo de ancho `s.width` con un `linearGradient` que va del color de la
  serie al color de la paleta de índice 4 (§5). El gradiente usa `gradientUnits="userSpaceOnUse"` y recorre el
  área de trazado de izquierda a derecha.
- **Marcadores.** Cada punto de una serie `scatter` lleva un halo (círculo de radio `size + 4`,
  `fill-opacity="0.18"`) y un marcador `<circle class="pt">` de radio `size`, con borde del color de fondo y
  trazo de 1.2.
- **Etiquetas de notas** en negrita, con el tamaño `label_size`.
- **Pie.** Si `caption` no está vacío, se dibuja en la esquina inferior izquierda, en cursiva y tamaño 11.

Las series de otros tipos (barras, cajas, mapas de calor, campos vectoriales, etc.) y las líneas de referencia
`hline` y `vline` no usan el trazado pulido: pasan por `draw_series`, como en el estilo por defecto.

## 4. Contrato de salida SVG

- La salida es **un único `<svg>`**, con su `</svg>` de cierre. Las pruebas comprueban que aparece una sola vez.
- El elemento raíz lleva `data-ostrin-polish="true"` solo cuando `polish` y `animated` son verdaderos **y** la
  figura no tiene series de movimiento. Si hay series de movimiento (`moving`, `trail`, `rod`, `morph`), la raíz
  lleva `data-ostrin-motion="true"` en su lugar y no se emite el atributo de pulido.
- Con `animate: true` se emite además una regla de estilo
  `@media (prefers-reduced-motion:reduce){[data-ostrin-polish] animate{display:none!important}}`, que oculta las
  animaciones si el usuario pide movimiento reducido.
- Los marcadores son `<circle class="pt" data-viz-index="i">`, con `i` el índice del punto dentro de su serie,
  desde 0. Es el selector que usan las pruebas para contar puntos.
- Los identificadores de gradiente se construyen así:
  - `polish_id = uid("polish", clip)`, donde `clip` es un identificador derivado del título, las etiquetas de
    ejes y el número de series;
  - la serie con índice 0 usa `polish_id`; las siguientes usan `polish_id + "-" + series_id`.
- El color de cada serie sale de `pick(color, index, theme)`. Si la serie no tiene color, se usa
  `theme.palette[index % theme.palette.length()]`. La paleta tiene 8 colores en el tema claro y 8 en el oscuro,
  así que el índice queda siempre dentro de los límites por el operador módulo.

## 5. Índice de paleta fijo en el gradiente

El extremo final del gradiente no usa el color de la serie, sino `pick("", 4, theme)`. Eso es el quinto color de la
paleta (índice base cero 4), no el cuarto. El comentario del código dice «fourth palette colour»; el comportamiento
real es el índice 4. Este documento describe el comportamiento real. Si se quiere el cuarto color, el cambio es
de una constante y debe acompañarse de la actualización de las pruebas.

## 6. Animación

Solo se anima cuando `animate` es verdadero. Hay dos tipos de movimiento, ambos SMIL:

- **Trazo de la línea.** Cada `line` pulido recibe un `<animate attributeName="stroke-dashoffset" from="1" to="0"
  dur="1.2s" fill="freeze">`. El trazo usa `pathLength="1"` y `stroke-dasharray="1 1"`, así que el dibujo
  no depende de la longitud real de la curva. Con `fill="freeze"` el estado final es la curva completa, y un
  renderizador estático (por ejemplo una imagen) muestra la curva entera con el desplazamiento 0.
- **Pulso de marcadores.** Cada marcador `scatter` recibe un `<animate attributeName="r"
  values="s;s+3;s" dur="2s" repeatCount="indefinite">`, donde `s` es el radio del marcador.

Ostrin no genera fotogramas intermedios: el navegador o el visor interpola los valores de SMIL.

## 7. Validación

Ejemplos (cada uno se compila y se ejecuta en el intérprete y en el backend nativo, con la misma salida):

| Ejemplo | Uso del estilo | Qué comprueba la prueba |
| --- | --- | --- |
| `examples/viz_polynomial_roots.ostrin` | pulido con `animate` | cuatro `<animate>`: tres pulsos de radio `5;8;5` y un trazo |
| `examples/viz_linear_system.ostrin` | pulido, con `animate` | un punto `pt`, y las etiquetas `x = 2, y = 1`, `2x + y = 5`, `x - y = 1` |
| `examples/viz_autodiff_derivative.ostrin` | pulido, dos series de línea | dos trazos con gradiente `url(#polish…)`, y la etiqueta `f'(x) (autodiff)` |
| `examples/viz_polished_sine_fit.ostrin` | pulido, marcadores | 41 marcadores `pt` |
| `examples/viz_polished_quadratic.ostrin` | pulido con `animate` | 41 marcadores `pt` |
| `examples/viz_polished_normal.ostrin` | pulido con `animate` | 41 marcadores `pt` |

Las pruebas que hacen esas comprobaciones están en `compiler/tests/examples.rs`:

- `std_viz_polish_matches_both_backends` (usa `viz_polynomial_roots.ostrin`);
- `std_viz_linear_system_matches_both_backends`;
- `std_viz_autodiff_derivative_matches_both_backends`;
- `std_viz_polished_sine_fit_matches_both_backends`;
- `std_viz_polished_quadratic_matches_both_backends`;
- `std_viz_polished_normal_matches_both_backends`.

Cada una compara la salida de los dos backends, cuenta elementos y busca cadenas exactas en el SVG. Las pruebas
también comprueban que el ejemplo no cae a un fallback del AST (`ast-fallback: 0`) cuando hay compilador de C
disponible. Esa comprobación está en `viz_polynomial_roots`, `viz_linear_system` y `viz_autodiff_derivative`, no en
los tres ejemplos `viz_polished_*`.

## 8. Límites actuales

- **`dash` se ignora en modo pulido.** `line(..., dash:)` guarda el valor, pero `draw_polish_line` no lo lee: el
  trazo de la línea usa siempre `stroke-dasharray="1 1"`.
- **Las animaciones se comprueban por valores SVG, no fotograma a fotograma.** Las pruebas verifican los
  atributos `values`, `dur`, `from`, `to` y `fill` en la salida. No hay una comprobación visual de la
  interpolación en un navegador.
- **Sin leyenda propia.** El modo pulido no dibuja una leyenda distinta: si `legend` está activa (lo está por
  defecto), se usa la misma función `legend` que en el estilo por defecto. El pie (`caption`) es el único texto
  adicional del estilo. Por tanto, «sin leyenda más allá del pie» no es correcto: la leyenda sigue apareciendo
  cuando está activa.
- **Identificadores de gradiente derivados del título.** `polish_id` sale de un hash del título, las etiquetas y
  el número de series. Dos figuras pulidas con el mismo título, etiquetas y número de series en una misma página
  HTML producirían los mismos `id`. Esto se deduce del código de `uid`; no hay una prueba que lo cubra.
- **Colores de paleta con índice fijo** en el gradiente (§5), con la discrepancia del comentario.
- **Solo `line` y `scatter`** usan el trazado pulido. El resto de series conserva su dibujo por defecto.

## 9. Lo que no está hecho

- No hay un parámetro para elegir el índice de paleta del gradiente ni para desactivar el área bajo la curva.
- No hay verificación visual automatizada del movimiento en un navegador.
- No hay un ajuste del estilo para figuras con series de movimiento: en ese caso la raíz usa
  `data-ostrin-motion` y el pulido se aplica solo a lo que no es movimiento.
