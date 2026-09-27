# 23 — Motor de visualización: `std.viz` 0.1

Estado: implementado (2026-09-24). Código: `compiler/std/viz.ostrin`. Ejemplos: `examples/viz_*.ostrin`,
`examples/lab_plot.ostrin`, `examples/lab_surface.ostrin`. Web: `website/viz.html` y las pestañas
Plot y 3D del Scientific Lab.

## 1. Objetivo y principios

La visualización es una capacidad principal de Ostrin, no un complemento. `std.viz` 0.1 fija la base
sobre la que crecerán la interacción, la animación y los backends acelerados:

1. **Escrito en Ostrin.** Ejes, ticks, leyendas, mapas de color, contornos, proyección 3D y
   sombreado son código Ostrin de la biblioteca estándar embebida. No hay un runtime gráfico oculto.
2. **Determinista y portable.** La misma figura produce los mismos bytes en el intérprete, en el
   backend nativo C y en `ostrinc.wasm` dentro del navegador. El test diferencial compara todos los
   `examples/viz_*.ostrin` byte a byte entre intérprete y nativo.
3. **Una figura es un valor.** `viz.figure(título)` y `viz.scene3d(título)` devuelven records; cada
   marca añade un `Series` a su lista. Renderizar es una función pura de ese valor (`fig.svg()`).
4. **Unidades en los ejes.** Los datos con cantidades físicas etiquetan sus ejes con la unidad que
   llevan (`speed [km/h]`); convertir los datos convierte el eje.
5. **Límites declarados.** Lo que no existe (contratos de eventos ricos, MP4, WebGPU) se dice en la
   web y aquí; nada se simula con JavaScript.

## 2. Arquitectura

```text
Figure / Scene3D (records)          ← API: figure(), scene3d(), métodos encadenables
   └── List<Series>                 ← modelo de escena: kind + arrays + estilo
          ↓ render / render3d
   layout (dominios, ticks 1-2-5, márgenes, leyenda)
   geometría (marching squares, cámara ortográfica, orden de pintor, luz)
          ↓
   SVG autocontenido (String)       ← único backend en 0.1
```

`Series` es la representación intermedia de la escena: `kind` (`line`, `scatter`, `area`, `band`,
`errorbar`, `bar`, `hist`, `boxplot`, `violin`, `hexbin`, `contourf`, `quiver`, `streamplot`, `stairs`, `hline`, `vline`, `heatmap`, `contour`, `surface`, `wire`,
`line3`, `scatter3`, `vector3`, `slice_xy`, `slice_xz`, `slice_yz`, `mesh3`), los arrays `xs`, `ys`, `zs`, `lo`, `hi`, `grid` y el estilo (`color`, `width`,
`dash`, `size`, `colormap`, `levels`). Como un `Array` de Ostrin no puede estar vacío, los campos que
una marca no usa contienen un cero y solo se leen para los `kind` que los rellenan.

Esta separación es la que permitirá añadir backends (Canvas, WebGPU, nativo) consumiendo la misma
lista de series, y un IR de visualización más formal cuando existan animación e interacción.

## 3. API

```ostrin
import std.viz

fig = viz.figure("Damped oscillator")          // 640 × 400, tema claro
    .describe("subtítulo")
    .labels("time t", "displacement x")
    .size(640, 400).dark().xlim(0.0, 10.0).ylim(-1.0, 1.0).no_legend()
    .line(xs, ys, label: "x(t)", color: "", width: 2.0, dash: "6 4")
    .scatter(xs, ys, label: "", size: 3.5)
    .area(xs, ys) .stairs(xs, ys) .band(xs, lo, hi) .errorbars(xs, ys, err)
    .bars(xs, heights) .histogram(data, bins)
    .boxplot(1.0, control, label: "control")  // repeat at each group position
    .violin(1.0, control, 40, label: "control") // Gaussian KDE shape + median
    .hexbin(xs, ys, 18, 14, "magma", "observations") // counted 2D density
    .contourf(z, -3.0, 3.0, -3.0, 3.0, 9, "magma", "z") // filled contour bands
    .quiver(xorig, yorig, u, v, 0.24, "velocity", "#2563eb") // sampled 2D vector field
    .streamplot(grid_x, grid_y, u_grid, v_grid, seed_x, seed_y, 72, 0.1, "flow", "#0f766e") // bidirectional streamlines
    .hline(y) .vline(x) .text(x, y, "nota")
    .heatmap(z, x0, x1, y0, y1, colormap: "viridis", label: "z")
    .contour(z, x0, x1, y0, y1, levels: 8, colormap: "", color: "")
    .unit_line(times, speeds)                    // Array<Quantity<X>>, Array<Quantity<Y>>
    .unit_scatter(times, speeds)
    .quantity_line(times, speeds)                // List<Quantity<X>>, List<Quantity<Y>>
    .quantity_scatter(times, speeds)
svg = fig.svg()
fig.save("figure.svg")                           // Result<Void, String>

scene = viz.scene3d("Surface").labels("x", "y", "z").view(-55.0, 28.0)
    .surface(xs, ys, z, colormap: "magma", label: "height")
    .wireframe(xs, ys, z) .line(xs, ys, zs, colormap: "viridis") .scatter(xs, ys, zs)

z = viz.grid_of(xs, ys, f)                       // z[i, j] = f(xs[j], ys[i])
page = viz.grid([a.svg(), b.svg()], 2, 480, 320, title: "Dashboard")
movie = viz.animate(frames, fps: 12.0)          // frames: List<String> de fig.svg()/scene.svg()

// Movimiento continuo (posiciones calculadas en Ostrin, muestras a pasos iguales de tiempo)
fig = viz.figure("Péndulo").no_axes().animate(12.0)          // duración del bucle en segundos
    .rod(0.0, 0.0, xs, ys)                        // segmento desde un punto fijo a un extremo móvil
    .moving_segment(ax, ay, bx, by)               // segmento con los dos extremos móviles
    .moving_point(xs, ys, trail: true, size: 7.0) // punto móvil; la estela se dibuja al avanzar
    .morph(xs, frames)                            // curva que cambia de forma (fila k = paso k)
```

Utilidades públicas: `viz.num` (dos decimales, idéntico en todos los backends), `viz.ticks`,
`viz.nice_step`, `viz.tick_label`, `viz.colormap(nombre, t, luz)`, `viz.argsort`, `viz.values` y
`viz.unit_of` para listas de cantidades, `viz.light()`/`viz.dark()`.

## 4. Algoritmos

- **Ticks**: paso 1, 2 o 5 × 10^k para ~8 (x) o ~6 (y) marcas; decimales según el paso; notación
  `me±k` fuera de [0.001, 100000).
- **Dominio**: unión de los datos (con `lo`/`hi` de bandas, barras de error y cajas, los bigotes de
  boxplots, el rango vertical de violines, medio paso de barras y bins); margen de 2 % (x) y 5 % (y) salvo en mapas de calor; las
  barras incluyen el cero.
- **Leyenda**: se evalúan las cuatro esquinas y se elige la que tapa menos puntos dibujados.
- **Heatmap**: cada valor es el centro de su celda, así los contornos coinciden con el mapa; las
  celdas se solapan 0.6 px para no dejar costuras.
- **Contornos**: marching squares sobre la rejilla, `levels` niveles equiespaciados.
- **Violines**: KDE gaussiana de ancho de banda de Silverman, muestreada en `bins` alturas y
  normalizada a una anchura común por grupo; la forma espejada y la mediana se escriben como SVG.
- **Hexbin**: los puntos se cuentan en una rejilla 2D y cada celda se dibuja como un hexágono
  con color mapeado, opacidad proporcional, tooltip de conteo y colorbar determinista.
- **Contornos rellenos**: `contourf` divide una rejilla en bandas discretas; cada celda toma la
  banda del promedio de sus cuatro esquinas, conserva un colorbar y un tooltip de valor, y puede
  combinarse con `contour` para dibujar las isolíneas sobre la misma figura.
- **Campos vectoriales 2D**: `quiver` conserva los orígenes y componentes `(u, v)` calculados por
  Ostrin, escala cada flecha en unidades de datos y dibuja línea, punta triangular y tooltip dentro
  del área recortada. Los vectores de longitud cero quedan como puntos, evitando geometría inválida.
- **Líneas de corriente 2D**: `streamplot` recibe ejes ascendentes y campos `u`/`v` aplanados por
  filas, interpola bilinealmente cada semilla y avanza en ambas direcciones con pasos de longitud
  de arco. Las trayectorias se detienen en el dominio o cuando la velocidad es nula; cada camino
  se mantiene como un `path.streamplot` determinista con tooltip.
- **3D**: datos normalizados al cubo [-1, 1]³ (z × 0.8), rotación por acimut y elevación, proyección
  ortográfica. Superficies como dos triángulos por celda, ordenados por profundidad (algoritmo del
  pintor con `argsort` estable) y sombreados con luz direccional: brillo 0.5 + 0.5 |n · l|. Suelo y
  paredes traseras con rejilla; etiquetas de ticks fuera del borde frontal.
- **Mapas de color**: viridis, magma, coolwarm y ocean, nueve paradas interpoladas linealmente.

## 4.1 Animación

`viz.animate(frames, fps)` recibe figuras ya renderizadas (cualquier mezcla de `Figure` y `Scene3D`)
y devuelve un único SVG del tamaño del primer fotograma. Cada fotograma va en un `<g
class="ostrin-frame">` con `animation-delay: i · paso`; una sola regla `@keyframes` lo deja opaco
durante `1/n` del ciclo (redondeado hacia arriba a milésimas de porcentaje, para que dos fotogramas
se solapen un instante en vez de dejar un hueco) y `step-end` evita fundidos. Los `id` (recortes,
degradados) se prefijan con `f<i>-` para que los fotogramas no compartan `clip-path`. Pasar el ratón
pausa la animación y `prefers-reduced-motion` muestra solo el primer fotograma.

Por qué CSS para la reproducción por defecto: CSS se ejecuta también en un `<img>` y el SVG sigue
siendo determinista byte a byte en intérprete, nativo y WASM. El explorador web ofrece play, pause,
reinicio, una barra temporal, control de velocidad, reproducción por uno o varios ciclos y exportación
WebM del número de ciclos elegido al abrir una figura animada; esos controles actúan sobre la copia del
SVG dentro del iframe y no cambian el programa Ostrin. La
exportación rasteriza
los fotogramas en un canvas y usa `MediaRecorder`, por lo que depende del soporte del navegador.
Coste: el tamaño crece linealmente con los fotogramas (24 fotogramas 2D ≈ 270 kB). El explorador
puede descargar el SVG completo y rasterizar el instante actual a PNG 2× (también un fotograma elegido
con la línea temporal). MP4 y calidad configurable siguen pendientes; PDF usa la impresión nativa del
navegador para conservar el vector sin introducir un conversor binario en el runtime.

## 4.2 Cortes ortogonales de volúmenes

Los volúmenes científicos se almacenan como `Array<Float>` de rango 3 con forma `[z, y, x]`.
`viz.slice_xy(volume, z)`, `viz.slice_xz(volume, y)` y `viz.slice_yz(volume, x)` extraen una
lámina 2D y recortan el índice a los límites del volumen. Los métodos encadenables
`Scene3D.slice_xy`, `.slice_xz` y `.slice_yz` colocan esas celdas en la escena 3D, las ordenan
con el algoritmo del pintor que usan las superficies, colorean por valor con una barra de escala
y añaden un tooltip SVG por celda. La ruta actual es un render CPU/SVG determinista; el volumen
completo queda para el backend WebGPU.

`Scene3D.isosurface(xs, ys, zs, volume, level, colormap:, label:)` polygoniza el mismo volumen
`[z, y, x]` con marching tetrahedra. Cada triángulo conserva sus tres vértices, se ordena por
profundidad y recibe iluminación plana, color y tooltip en SVG. Es una superficie explícita y
reproducible para volúmenes medianos; el render volumétrico denso sigue reservado para WebGPU.

## 4.3 Movimiento continuo

`fig.animate(segundos)` convierte una figura en animación. Las marcas de movimiento guardan una
posición por paso de tiempo, todas calculadas por el programa, y el SVG las interpola con SMIL
(`<animate attributeName="cx" values="…" dur="12s" repeatCount="indefinite">`). Los atributos
estáticos llevan la primera muestra, así que un visor sin animación muestra el instante inicial.

- `moving_point`: anima `cx`/`cy`. Con `trail`, el recorrido completo es un `<path>` con
  `pathLength="1000"` y `stroke-dasharray="1000 1000"`; `stroke-dashoffset` sigue la distancia
  recorrida en píxeles hasta cada muestra, así que la estela crece al ritmo real del punto (no a
  velocidad constante).
- `rod`, `moving_segment`: animan `x2`/`y2` (y `x1`/`y1` si el origen también se mueve).
- `morph`: una fila de `frames` por paso. Todas las rutas tienen los mismos comandos (`M` + `L`), así
  que SMIL interpola la forma punto a punto.

Frente a `viz.animate` (fotogramas), el movimiento continuo es suave a cualquier velocidad de
refresco y mucho más ligero: 12 s de doble péndulo a 60 muestras/s ocupan 72 kB, frente a unos
270 kB para 24 fotogramas 2D. Los fotogramas siguen siendo la vía para lo que SMIL no puede
interpolar, como reordenar los triángulos de una superficie 3D que gira.

Ejemplos: `viz_double_pendulum` (rk45 con deriva de energía < 1e-6), `viz_orbits` (Kepler, en UA y
años), `viz_string` (cuerda pulsada, 25 modos) y la pestaña ODE del Lab (péndulo y retrato de fase
animados, recalculados en el navegador al mover los controles). `no_axes()` oculta ticks, rejilla y
marco para escenas tipo "escenario".

En el SVG, `@media (prefers-reduced-motion: reduce)` oculta los elementos SMIL y deja visibles sus
valores iniciales; el explorador además pausa la copia del documento y anuncia el modo reducido.
Límite conocido: en Chromium cada `<svg>` anidado (paneles de `viz.grid`) tiene su propio reloj, que
arranca con la carga, así que los paneles quedan sincronizados en la reproducción normal.

## 4.4 Tablas reproducibles

`viz.table(headers, rows, title:)` convierte encabezados y filas de texto preparados por el programa
en un SVG determinista. El renderer calcula anchos de columna a partir del contenido, limita el
ancho de cada columna para mantener la figura manejable, alterna el fondo de las filas y añade un
`<title>` por celda para inspeccionar el valor completo al pasar el ratón. `dark()`, `size()` y
`row_height()` permiten adaptar la presentación sin introducir un formato de datos oculto: el
programa sigue siendo responsable de convertir números, unidades y precisión. La misma tabla se
puede abrir como SVG, guardarse con `save()` y reproducirse en intérprete, nativo y WASM. El
explorador web identifica las filas y columnas producidas por Ostrin y permite filtrar texto y
ordenar columnas numéricas o textuales sin recalcular ni dibujar datos fuera de la salida SVG; el
pie de tabla informa cuántas filas quedan visibles. Las figuras compuestas con `viz.grid` pueden
enlazar puntos y filas mediante el marcador determinista `data-viz-index`; el explorador instala
selección por clic y teclado y resalta ambas vistas dentro del iframe aislado.

## 4.5 Cámara 3D en el explorador web

Las escenas 3D de la galería exponen controles de acimut y elevación en el explorador. Cada cambio
reemplaza el `.view(azimuth, elevation)` del ejemplo y ejecuta de nuevo el programa con
`ostrinc.wasm`; el SVG resultante se vuelve a mostrar en el iframe aislado y se guarda como la
versión viva de la tarjeta. El botón de reinicio recupera los ángulos declarados por el código fuente.
La interfaz anuncia el estado de renderizado y conserva el teclado y el modo de movimiento reducido.
JavaScript coordina la interacción, mientras que Ostrin calcula la geometría y produce la figura.

## 4.6 Exportación desde el explorador

El explorador conserva dos rutas de publicación para cada figura:

- **SVG** descarga exactamente el documento producido por Ostrin, incluidos sus datos, tooltips y
  animaciones declarativas.
- **PNG** convierte el SVG estático del instante actual en un canvas a escala 2× y descarga un archivo
  compatible con editores, informes y mensajería. Para una animación, la posición de la línea temporal
  determina el fotograma rasterizado; la exportación no depende de que el iframe permita scripts.

La rasterización vive en el navegador y tiene una ruta de error explícita cuando el contexto 2D o la
codificación PNG no están disponibles. **PDF** abre una vista de impresión con el fotograma estático actual,
el tamaño calculado del SVG y la línea de procedencia; el usuario elige `Save as PDF` en el diálogo nativo.
Así el archivo conserva los vectores, el nodo `<ostrin-provenance>` y el tamaño de página sin subir datos a
un servidor ni convertir la salida de Ostrin en una imagen raster. Si el navegador bloquea ventanas emergentes,
el explorador anuncia el error y no oculta el SVG original.

**GIF** reutiliza los mismos fotogramas estáticos que WebM, los cuantiza a una paleta GIF determinista y
los comprime con LZW en el navegador. El archivo incluye un comentario con la procedencia (`source-hash`,
`data-hash`, `seed` y `compiler`) cuando la figura la declara. El límite práctico es el tamaño de los GIF
grandes; SVG y WebM siguen siendo preferibles para publicación y alta resolución.

**HTML interactivo** empaqueta el fotograma SVG seleccionado, la procedencia y una pequeña interfaz
autocontenida en un archivo descargable. El documento funciona sin el sitio de Ostrin ni una CDN:
permite zoom, desplazamiento con el puntero y un crosshair accesible que vuelve a leer los títulos
de los elementos producidos por Ostrin. La figura continúa siendo el SVG original sanitizado; el
navegador solo aporta la interacción de inspección y navegación. Esto convierte una figura reproducible
en un artefacto que se puede adjuntar a un informe o abrir localmente.

### 4.6.1 Parámetros conducidos por Ostrin

Una figura de la galería puede declarar controles numéricos junto a su programa. Cada control apunta a
una asignación simple (`nombre = literal`) del código fuente, conserva el tipo literal y sustituye solo
ese valor antes de volver a ejecutar `ostrinc.wasm`. La salida sigue siendo el SVG producido por Ostrin;
el navegador solo presenta el control, aplica el valor y muestra el estado de la ejecución. La superficie
3D expone `scale` y el atractor de Lorenz expone `rho` como ejemplos verificables. Los contratos de eventos
entre varias figuras todavía requieren una API explícita.

## 4.7 Procedencia reproducible

`Figure`, `Scene3D` y `Table` aceptan `.provenance(source_hash, data_hash, seed, compiler)`.
El método no recalcula hashes ni inventa una fuente: el programa o su pipeline de construcción pasa
los identificadores verificables del experimento. `seed` puede incluir la fecha de ejecución y la
semilla usada por el generador aleatorio; `compiler` conserva la versión exacta de `ostrinc`.

El renderizador escribe esos cuatro valores como un nodo `<metadata><ostrin-provenance .../>` dentro
del SVG. Por eso sobreviven a la descarga, a la exportación PNG/WebM que parte del SVG y a la
reproducción en intérprete, C nativo y WASM. La galería lee el nodo y lo muestra en cada tarjeta,
incluido el resultado recalculado con **Run live**. Un archivo sin `.provenance(...)` sigue siendo
válido y declara que no tiene registro de procedencia.

## 5. Unidades

`quantity_line(xs, ys)` acepta `List<Quantity<X>>` y `List<Quantity<Y>>`: toma los números en la
unidad del primer elemento (`q.value()`, convirtiendo los que usen otra unidad) y guarda
`q.unit()` para el título del eje (`time [s]`). Requirió dos capacidades del lenguaje:

- `q.value()` y `q.unit()` sobre `Quantity` (intérprete y C), y `unit` como nombre de miembro;
- inferencia de parámetros de dimensión en métodos genéricos (`Quantity<X>` con `X: Dimension`).

Con arrays de cantidades (`Array<Quantity<D>>`) se usan `unit_line`/`unit_scatter`, que toman
`a.values()` y `a.unit()` directamente.

## 6. Hallazgos del compilador

Construir la biblioteca destapó y corrigió: ámbito dinámico accidental en el intérprete (una función
reasignaba variables del llamador), métodos que ignoraban argumentos nombrados y por defecto,
reescritura de módulos que confundía parámetros con funciones del módulo, sentencias con resultado
propio evaluadas dos veces en C, asignaciones dentro de bucles que dejaban punteros colgantes en C, y
la falta de literales científicos (`1e-9`). Ver `CONTEXTO_PROYECTO.md` §270–272.

## 7. Límites y siguientes versiones

| Versión | Contenido |
|---|---|
| 0.1 (hecho) | marcas 2D, heatmap/contornos, superficies/trayectorias/nubes 3D, campos vectoriales muestreados, layouts, unidades en ejes, SVG |
| 0.1 (hecho) | boxplots agrupados con cuartiles interpolados, mediana, bigotes, tooltips y leyenda; violines KDE gaussianos con marcador de mediana |
| 0.1 (hecho) | hexbin 2D con conteo de puntos, celdas hexagonales color-mapeadas, tooltips y colorbar |
| 0.1 (hecho) | `contourf` con bandas discretas rellenas, colorbar, tooltips y combinación con isolíneas |
| 0.1 (hecho) | `quiver` con campos vectoriales 2D muestreados, puntas triangulares, escala en unidades de datos y tooltips |
| 0.1 (hecho) | `streamplot` con interpolación bilineal, integración bidireccional desde semillas, recorte al dominio y tooltips |
| 0.2 (parcial, hecho) | tooltips `<title>` con valores y resaltado CSS al pasar el ratón, dentro del SVG y sin scripts; visor web con zoom y desplazamiento en un iframe aislado |
| 0.2 (parcial, hecho) | selección enlazada entre puntos y filas en figuras compuestas; parámetros declarativos de figuras se reevalúan con Ostrin en WASM |
| 0.2 (parcial, hecho) | leyenda con identificadores de serie y controles accesibles en el explorador web para ocultar/mostrar series sin alterar los datos ni la exportación fuente |
| 0.2 (parcial, hecho) | inspección de datos con una retícula (`crosshair`) superpuesta a las marcas SVG y lectura de sus tooltips, activable con teclado y respetando el aislamiento del iframe |
| 0.3 (parcial, hecho) | animación en bucle con `viz.animate`: fotogramas generados por Ostrin, reproducidos con CSS dentro del SVG |
| 0.3 (hecho) | movimiento continuo con SMIL: `animate`, `moving_point` con estela, `rod`, `moving_segment`, `morph`; `no_axes` |
| 0.3 (parcial, hecho) | controles web de play/pausa/reinicio, posición temporal, velocidad, ciclos finitos y exportación WebM cuando el navegador ofrece `MediaRecorder` |
| 0.3 (hecho) | descarga del SVG producido y exportación PNG 2× del fotograma actual desde el explorador web |
| 0.4 (hecho) | vista de impresión PDF con fotograma vectorial, tamaño de página y procedencia; el diálogo nativo permite guardar el PDF |
| 0.4 (hecho) | exportación GIF con fotogramas cuantizados, compresión LZW y comentario de procedencia |
| 0.4 (hecho) | exportación HTML autocontenida del fotograma SVG con procedencia, zoom, desplazamiento y crosshair |
| 0.3 (hecho) | tablas SVG reproducibles con filas alternadas, encabezados, tooltips por celda, tema oscuro y explorador web con filtro/ordenamiento (`viz.table`) |
| 0.4 (parcial, hecho) | campos vectoriales 3D muestreados con flechas, profundidad y tooltips |
| 0.4 (parcial, hecho) | explorador web con acimut/elevación que vuelve a ejecutar `.view(...)` en WASM |
| 0.4 (parcial, hecho) | cortes XY/XZ/YZ e isosuperficies de `Array<Float>` 3D, celdas/triángulos coloreados, tooltips y barra de escala; cámaras ortográficas interactivas |
| 0.4 | render de volúmenes completos, isosuperficies y cámaras en perspectiva |
| 0.5 | backend WebGPU sobre la misma lista de series; MP4 y calidad configurable |
| 0.5 (hecho) | figuras, tablas y escenas con procedencia (`source-hash`, `data-hash`, `seed`, `compiler`) embebida en SVG y mostrada en la galería |
| — | gráficas con incertidumbre cuando exista `Measurement<T>` |

Límites actuales: renderizado SVG en CPU, cómodo hasta unos miles de triángulos; los SVG 3D grandes
pesan cientos de kB; la galería nativa completa deja ~300 asignaciones vivas al salir (textos de
unidades); el pico de memoria es ~4 000 asignaciones.
