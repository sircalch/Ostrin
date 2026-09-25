# Preparación para GitHub Linguist

Ostrin todavía no aparece como lenguaje oficial en GitHub. Esta página separa lo
que ya está preparado en el repositorio de lo que depende de uso público y de la
aceptación de `github/linguist`.

## Entrada propuesta

La entrada que se llevaría a `lib/linguist/languages.yml` es:

```yaml
Ostrin:
  type: programming
  color: "#5B3DF5"
  aliases:
  - ostrin
  extensions:
  - ".ostrin"
  interpreters:
  - ostrinc
  tm_scope: source.ostrin
  ace_mode: text
```

El `language_id` no se añade manualmente; Linguist lo genera con
`script/update-ids`. `source.ostrin` corresponde a la gramática TextMate que ya
usa la extensión de VS Code en [`vscode-ostrin/syntaxes/ostrin.tmLanguage.json`](../vscode-ostrin/syntaxes/ostrin.tmLanguage.json).
La gramática y las muestras de la propuesta se publicarían bajo la licencia MIT
del proyecto.

## Muestras candidatas

La propuesta debe llevar muestras reales y representativas, no un `hello world`
aislado. El repositorio ya contiene ejemplos de:

- cantidades y conversiones: `examples/physics.ostrin`, `examples/quantity_arrays.ostrin`;
- arrays y métodos numéricos: `examples/arrays.ostrin`, `examples/numeric_methods.ostrin`;
- concurrencia: `examples/native_concurrency.ostrin`, `examples/concurrency.ostrin`;
- records, traits, módulos y paquetes: los ejemplos correspondientes bajo `examples/`.

El inventario actual tiene 246 archivos `.ostrin`, pero todos pertenecen al
repositorio de Ostrin. Eso demuestra variedad sintáctica, no el uso distribuido
que Linguist exige para aceptar una extensión nueva.

## Requisito externo antes de enviar

La guía actual de Linguist pide una búsqueda de GitHub que demuestre al menos
2.000 archivos públicos por extensión cuando puede aparecer más de una vez por
repositorio, además de una distribución razonable entre repositorios y usuarios.
La extensión `.ostrin` todavía no tiene esa evidencia independiente, por lo que
abrir ahora una propuesta upstream tendría alta probabilidad de rechazo. El
repositorio no debe añadir un `.gitattributes` que simule reconocimiento oficial:
Linguist no incluye lenguajes ausentes de `languages.yml` en sus estadísticas.

## Procedimiento cuando exista uso suficiente

1. Preparar una copia de muestras representativas bajo `samples/Ostrin/` y
   conservar sus licencias u origen.
2. Añadir la entrada anterior y la gramática mediante las herramientas de
   `github/linguist`.
3. Ejecutar `bundle exec rake test` y
   `bundle exec script/cross-validation --test` dentro del checkout de Linguist.
4. Abrir el PR upstream con la búsqueda de GitHub, las licencias de las muestras
   y la plantilla completa.
5. Esperar la aceptación y una versión publicada de Linguist; solo entonces
   comprobar que GitHub clasifica `.ostrin` y actualizar el estado del proyecto.

La referencia normativa es la guía de contribución de
[`github/linguist`](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md).
