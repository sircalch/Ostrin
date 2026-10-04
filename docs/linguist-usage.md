# Evidencia reproducible de uso público para GitHub Linguist

Este documento registra la evidencia externa necesaria antes de enviar la propuesta
de Ostrin a [`github/linguist`](https://github.com/github-linguist/linguist). El reporte
no cuenta los ejemplos del repositorio `sircalch/Ostrin` y no afirma que GitHub ya
reconozca `.ostrin` como lenguaje.

## Snapshot actual

**Fecha del snapshot:** 2026-09-30

| Campo | Resultado |
| --- | --- |
| Consulta | [`NOT is:fork extension:ostrin -repo:sircalch/Ostrin`](https://github.com/search?q=NOT%20is%3Afork%20extension%3Aostrin%20-repo%3Asircalch%2FOstrin&type=code) |
| Archivos públicos `.ostrin` indexados | **0** |
| Umbral de Linguist para una extensión común | **2.000** archivos en el último año |
| Distribución entre repositorios y usuarios | Pendiente de revisión manual |
| Propuesta upstream | **No lista para abrirse** |

Los **282** archivos `.ostrin` rastreados en el repositorio propio (**266** bajo `examples/`)
son muestras de sintaxis y capacidad del lenguaje. No se suman a esta evidencia de uso público. Mientras el
contador sea 0 no se debe abrir un PR en Linguist ni presentar el mapa de lenguajes
como si ya incluyera Ostrin.

## Generar el reporte

Requisitos: [GitHub CLI](https://cli.github.com/) instalado y autenticado.

```powershell
gh auth login
node scripts/linguist-usage-report.mjs
node scripts/linguist-usage-report.mjs --json
node scripts/linguist-usage-report.mjs --json --output .tmp/linguist-usage.json
```

La consulta se ejecuta explícitamente como:

```text
gh api --method GET search/code -f q='NOT is:fork extension:ostrin -repo:sircalch/Ostrin'
```

El script falla si `gh` no está instalado, si no existe una sesión autenticada, si
GitHub devuelve una respuesta inválida o si la respuesta incluye accidentalmente el
repositorio propio. El artefacto marca los resultados incompletos y deja la revisión
de forks, usuarios y repositorios únicos como evidencia manual; no convierte un
contador parcial en aprobación.

## Criterios de envío upstream

La guía de contribución de Linguist exige, para una extensión que puede aparecer
varias veces por repositorio, al menos **2.000 archivos indexados en el último año**,
excluyendo forks, además de una distribución razonable entre combinaciones únicas de
usuario/repositorio. También exige muestras reales, sus licencias y enlaces al código
original. La cifra por sí sola no es suficiente.

Antes de abrir el PR se deben conservar en el informe:

1. la URL y la fecha de la consulta de GitHub;
2. el total indexado y el indicador `incomplete_results`;
3. una revisión manual de la distribución entre repositorios y usuarios;
4. la exclusión de forks y del repositorio `sircalch/Ostrin`;
5. la licencia y el origen de cada muestra propuesta.

La entrada local en [`docs/linguist-language.yml`](linguist-language.yml), la gramática
TextMate y el inventario source-backed de muestras en
[`docs/linguist-samples.yml`](linguist-samples.yml) pasan el gate local:

```powershell
node scripts/linguist-check.mjs
node --test scripts/linguist-check.test.mjs
```

Ese gate comprueba la preparación reproducible del repositorio, incluidas las rutas,
licencias y enlaces de origen de las muestras; no reemplaza la aceptación de Linguist
ni la evidencia de uso externo.

## Validación contra un checkout real de `github-linguist`

Cuando el umbral externo se cumpla, usar un checkout limpio del proyecto oficial:

```powershell
git clone https://github.com/github-linguist/linguist.git
Set-Location linguist
script/bootstrap
```

En ese checkout:

1. añadir la entrada de `Ostrin` a `lib/linguist/languages.yml` sin inventar
   `language_id`;
2. incorporar la gramática TextMate con `script/add-grammar` y conservar su licencia;
3. copiar muestras representativas a `samples/Ostrin/`, con enlaces a su origen y
   licencia MIT del proyecto cuando corresponda;
4. ejecutar `script/update-ids`;
5. ejecutar `bundle exec rake test`;
6. ejecutar `bundle exec script/cross-validation --test`;
7. abrir el PR usando la plantilla oficial y adjuntar la búsqueda de GitHub, la
   distribución revisada y el inventario de licencias.

Después de una eventual aceptación, esperar una versión publicada de Linguist y
verificar la clasificación de `.ostrin` en GitHub. Las estadísticas de un repositorio
se actualizan mediante un trabajo en segundo plano y pueden conservar caché hasta que
el repositorio reciba una actualización. Solo después se actualizan la release, el
sitio y [`docs/linguist.md`](linguist.md).

## Política del repositorio

No añadir una regla `.gitattributes` para simular reconocimiento oficial. Linguist
solo incluye lenguajes definidos en su `languages.yml`, y el proyecto debe mantener
la diferencia entre:

- preparación local comprobada;
- uso público suficiente;
- PR upstream aceptado;
- versión publicada y clasificación visible en GitHub.

Fuentes normativas: [guía de contribución de Linguist](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md),
[cómo funciona Linguist](https://github.com/github-linguist/linguist/blob/main/docs/how-linguist-works.md)
y [documentación de overrides](https://github.com/github-linguist/linguist/blob/main/docs/overrides.md).

