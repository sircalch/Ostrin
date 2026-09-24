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
5. **Límites declarados.** Lo que no existe (interacción, animación, PNG/PDF, WebGPU) se dice en la
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
`errorbar`, `bar`, `hist`, `stairs`, `hline`, `vline`, `heatmap`, `contour`, `surface`, `wire`,
`line3`, `scatter3`), los arrays `xs`, `ys`, `zs`, `lo`, `hi`, `grid` y el estilo (`color`, `width`,
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
```

Utilidades públicas: `viz.num` (dos decimales, idéntico en todos los backends), `viz.ticks`,
`viz.nice_step`, `viz.tick_label`, `viz.colormap(nombre, t, luz)`, `viz.argsort`, `viz.values` y
`viz.unit_of` para listas de cantidades, `viz.light()`/`viz.dark()`.

## 4. Algoritmos

- **Ticks**: paso 1, 2 o 5 × 10^k para ~8 (x) o ~6 (y) marcas; decimales según el paso; notación
  `me±k` fuera de [0.001, 100000).
- **Dominio**: unión de los datos (con `lo`/`hi` de bandas y barras de error, medio paso de barras y
  bins); margen de 2 % (x) y 5 % (y) salvo en mapas de calor; las barras incluyen el cero.
- **Leyenda**: se evalúan las cuatro esquinas y se elige la que tapa menos puntos dibujados.
- **Heatmap**: cada valor es el centro de su celda, así los contornos coinciden con el mapa; las
  celdas se solapan 0.6 px para no dejar costuras.
- **Contornos**: marching squares sobre la rejilla, `levels` niveles equiespaciados.
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

Por qué CSS y no SMIL ni JavaScript: CSS se ejecuta también en un `<img>`, no requiere scripts (la
regla de la web: JavaScript no produce resultados) y el SVG sigue siendo determinista byte a byte en
intérprete, nativo y WASM. Coste: el tamaño crece linealmente con los fotogramas (24 fotogramas 2D
≈ 270 kB). Pendiente: reproducir una vez, barra de desplazamiento temporal y exportar vídeo.

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
| 0.1 (hecho) | marcas 2D, heatmap/contornos, superficies/trayectorias/nubes 3D, layouts, unidades en ejes, SVG |
| 0.2 (parcial, hecho) | tooltips `<title>` con valores y resaltado CSS al pasar el ratón, dentro del SVG y sin scripts; visor web con zoom y desplazamiento en un iframe aislado |
| 0.2 (resto) | selección enlazada y controles conducidos por Ostrin (requiere un backend con eventos) |
| 0.3 (parcial, hecho) | animación en bucle con `viz.animate`: fotogramas generados por Ostrin, reproducidos con CSS dentro del SVG |
| 0.3 (resto) | reproducir una vez, control temporal, exportar vídeo |
| 0.4 | volúmenes, isosuperficies, campos vectoriales, cortes; cámaras en perspectiva |
| 0.5 | backend WebGPU sobre la misma lista de series; PNG/PDF |
| — | figuras con procedencia (hash de fuente y datos, semilla, versión del compilador) |
| — | gráficas con incertidumbre cuando exista `Measurement<T>` |

Límites actuales: renderizado SVG en CPU, cómodo hasta unos miles de triángulos; los SVG 3D grandes
pesan cientos de kB; la galería nativa completa deja ~300 asignaciones vivas al salir (textos de
unidades); el pico de memoria es ~4 000 asignaciones.
