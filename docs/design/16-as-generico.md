# Ostrin — Cierre del pendiente: `as D` (conversión genérica de escalares a cantidades)

Versión: 0.1
Depende de: [01-variables-tipos-unidades.md](01-variables-tipos-unidades.md), [02-funciones-y-firmas.md](02-funciones-y-firmas.md)

Este pendiente venía arrastrándose desde el documento 02 (§3.3), donde el ejemplo de `average<D: Dimension>` usaba una construcción `values.length() as D` nunca definida formalmente. Al sentarse a diseñarla en serio, resultó que **no hacía falta ningún mecanismo nuevo** — el ejemplo original tenía dos problemas distintos, y arreglarlos por separado hace desaparecer el pendiente sin añadir sintaxis.

---

## 1. El primer problema: faltaba una regla aritmética

El documento 01 (§3.3) nunca definió qué pasa al dividir una `Quantity<D>` entre un escalar puro (`Int`/`Float`) — solo estaba la regla simétrica de la multiplicación (`Quantity<A> * Int/Float` escala el valor, conserva la dimensión). Sin esa regla, dividir `sum(values) / values.length()` directamente no tenía una regla que lo permitiera, y de ahí venía la tentación de "convertir primero `values.length()` a una `Quantity<D>`" para poder usar la regla de división `Quantity/Quantity` que sí existía.

**Corrección aplicada** (documento 01, §3.3): se añadió la regla que faltaba —

```text
Quantity<A> / Int/Float → OK. Escala el valor (divide), conserva la dimensión A.
```

Dividir entre un escalar puro es solo un cambio de magnitud, exactamente igual de válido que multiplicar por uno — no hay ninguna razón por la que solo la multiplicación estuviera permitida.

## 2. El segundo problema: la conversión propuesta era, además, incorrecta

Incluso si `as D` hubiera existido como mecanismo, usarlo en `average` como `values.length() as D` (convirtiendo el conteo a `Quantity<D>` para luego dividir `Quantity<D> / Quantity<D>`) habría sido un **error semántico**, no solo una construcción de más: dividir dos cantidades de la misma dimensión **cancela la dimensión a `Float` puro** (documento 01, §3.3 — es la misma regla que hace que `velocity = a / b` con `a`, `b` ambas `Length` dé un número sin unidad, no una `Quantity`). Aplicado a `average`, eso habría devuelto un `Float` sin unidad como "promedio", perdiendo exactamente la información (la unidad) que la función debía conservar. El error no era de sintaxis — era que la operación propuesta, de haber compilado, habría dado el resultado equivocado.

Con la regla de §1 (`Quantity<D> / Int` conserva `D`), la versión correcta es simplemente:

```ostrin
fn average<D: Dimension>(values: List<Quantity<D>>) -> Quantity<D> {
    sum(values) / values.length()
}
```

Ya corregido en el documento 02.

## 3. ¿Y si de verdad hiciera falta inyectar una dimensión genérica en un escalar?

La pregunta original planteaba un caso más general: una función genérica sobre `D` que necesita convertir un número puro en una `Quantity<D>` — no solo en el caso de `average`. Revisando el documento 01 (§2.2.1, ya introducido en la primera revisión de consistencia), esto **ya estaba resuelto sin saberlo**: un símbolo de unidad es, por sí mismo, una expresión de tipo `Unit<D>` — y `as <unidad>` (documento 01, §3.3) nunca estuvo restringido a un token literal escrito a mano; acepta **cualquier expresión de tipo `Unit<D>`**, incluida una recibida como parámetro:

```ostrin
fn inject<D: Dimension>(raw: Float, target_unit: Unit<D>) -> Quantity<D> {
    raw as target_unit
}

inject(5.0, nm)    // Quantity<Length>, D resuelto a Length por el argumento 'nm'
inject(3.0, s)     // Quantity<Time>, D resuelto a Time por el argumento 's'
```

(El parámetro se llama `target_unit`, no `unit` — `unit` es palabra reservada para declarar unidades nuevas, documento 01 §3.5, y no puede reusarse como nombre de variable; ver el registro de palabras reservadas en el documento 10.)

Esto sí es genuinamente genérico sobre `D` — pero nótese que **siempre hace falta una unidad concreta** (`nm`, `s`, o lo que sea, pasado como valor `Unit<D>`) para construir la `Quantity`. No existe, ni tiene sentido semántico, un `as D` que inyecte "la dimensión `D` en abstracto" sin ninguna unidad concreta asociada — una cantidad física siempre se expresa en alguna unidad (aunque sea la unidad base de esa dimensión); "5 en dimensión `Length`, sin decir en qué unidad" no es un valor construible, es una contradicción del propio modelo del documento 01 (§2.2, `Quantity<D>` guarda su unidad de expresión). Esta ausencia de un `as D` "puro" no es una limitación pendiente de resolver: es la razón de fondo por la que la construcción original nunca hizo falta.

## 4. Conclusión

El pendiente "`as D`" se cierra sin añadir ningún elemento nuevo al lenguaje:
- Se corrigió una regla aritmética que faltaba en el documento 01.
- Se corrigió el ejemplo `average` del documento 02, que además de innecesariamente complejo era semánticamente incorrecto.
- Se dejó explícito (documento 01, §3.3, edición) que `as` ya generalizaba al caso de conversión genérica vía un parámetro `Unit<D>`, sin necesitar sintaxis adicional.

No quedan preguntas abiertas de este tema.
