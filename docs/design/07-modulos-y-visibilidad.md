# Ostrin — Diseño core: Módulos y Visibilidad

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: todos los documentos anteriores (01–06)

Decisiones de fondo ya cerradas:
- **Privado por defecto**; hay que marcar `pub` para exponer algo fuera de su módulo.
- **Acceso calificado por defecto** (`modulo.nombre`); traer nombres al alcance local sin prefijo es un paso explícito, no automático.

---

## 1. Qué es un módulo

- **Un archivo `.ostrin` es un módulo.** No existe una palabra clave `module` para delimitarlo — el archivo mismo es la unidad.
- **Un directorio es un espacio de nombres** que agrupa los módulos (archivos) que contiene, y puede a su vez contener subdirectorios. No hace falta ningún archivo especial para que un directorio "exista" como espacio de nombres.
- El **path de un módulo** se deriva de su ruta relativa a la raíz del proyecto, reemplazando separadores de carpeta por `.` y quitando la extensión:

```text
proyecto/
├── main.ostrin
├── physics/
│   ├── units.ostrin
│   └── particles.ostrin
└── math/
    └── linear_algebra.ostrin
```

```text
physics/units.ostrin           → módulo "physics.units"
physics/particles.ostrin       → módulo "physics.particles"
math/linear_algebra.ostrin     → módulo "math.linear_algebra"
```

## 2. Visibilidad

```ostrin
// physics/units.ostrin

fn internal_conversion_table() -> Map<String, Float> {   // privado: solo visible dentro de este archivo
    ...
}

pub fn to_kelvin<D: Dimension>(x: Quantity<D>) -> Quantity<D> {   // pub: visible desde cualquier módulo que importe physics.units
    ...
}

pub record Particle {
    pub mass: Quantity<Mass>
    pub charge: Quantity<Charge>
}
```

- `pub` se escribe antes de `fn`, `record`, `enum`, `trait`, o de un campo individual dentro de un `record`/`enum` — la visibilidad de un campo es independiente de la del tipo que lo contiene (un `record` puede ser `pub` con algunos campos privados, para exponer el tipo sin exponer su representación interna completa).
- Solo hay **dos niveles**: privado al archivo, o público a todo el proyecto (y a quien importe el paquete, una vez exista empaquetado — pendiente, ver §5). No hay un nivel intermedio tipo "visible solo dentro de esta carpeta" en esta versión del diseño: se prefiere mantener el modelo simple mientras no haya evidencia de que dos niveles no alcanzan (ver pendiente en §6).
- Sin `pub`, un nombre no existe fuera de su propio archivo, ni siquiera para otro archivo en el mismo directorio.

## 3. Importar

```ostrin
import physics.units

d = 5 * units.meter
```

- `import <path.de.modulo>` trae el módulo al alcance **como espacio de nombres calificado**, nombrado por su último segmento (`units`, no `physics.units`, para no repetir el prefijo completo en cada uso).
- Solo los símbolos marcados `pub` en `physics.units` son accesibles vía `units.algo`; intentar acceder a un símbolo privado desde otro módulo es error de compilación (no un problema de "no encontrado" ambiguo — el compilador indica que existe pero es privado):

```text
Error OSTRIN-E1080
'internal_conversion_table' is private to module 'physics.units'.
Only symbols marked 'pub' are accessible from other modules.
```

### 3.1 Alias

```ostrin
import physics.units as u

d = 5 * u.meter
```

Útil cuando el nombre por defecto choca con otro import o es demasiado largo en uso repetido.

### 3.2 Traer nombres específicos al alcance local

```ostrin
import physics.units.{meter, second}

d = 5 * meter
t = 10 * second
```

- Solo trae exactamente los nombres listados, sin prefijo.
- **No existe import con comodín** (no hay equivalente a `import physics.units.*`): cada nombre que entra al alcance sin calificar queda explícito en el propio `import`, así el origen de cualquier identificador se puede rastrear leyendo el bloque de imports del archivo, sin tener que buscar en la definición de cada módulo importado.

## 4. Re-exportar (`pub import`)

Para que un módulo actúe como fachada pública de varios módulos internos:

```ostrin
// physics/mod.ostrin  (o cualquier archivo que se quiera usar como "punto de entrada" del paquete)

pub import physics.units.{meter, second, kelvin}
pub import physics.particles.{Particle}
```

Quien importe `physics` (el archivo anterior) ve `meter`, `second`, `kelvin`, `Particle` como si fueran suyos, sin necesidad de conocer que en realidad viven en `physics.units`/`physics.particles`. Esto permite reorganizar la estructura interna de archivos de una librería sin romper el código de quien la usa, siempre que la fachada pública (`pub import`) se mantenga estable.

## 5. Ciclos de importación

Dos módulos que se importan mutuamente de forma directa (`A` importa `B` y `B` importa `A`) son **error de compilación**. El grafo de dependencias entre módulos debe ser un DAG (grafo acíclico dirigido):

```text
Error OSTRIN-E1081
Circular import detected:
    physics.particles → physics.forces → physics.particles
Break the cycle by moving the shared definitions into a third module
that both can import.
```

Esto es intencional y no una limitación temporal: simplifica el orden de inicialización de módulos y hace que la estructura de dependencias sea legible como un árbol, no un grafo arbitrario. (Dentro de un mismo archivo, la recursión mutua entre funciones sigue permitida sin restricción — documento 02, §6 — esto solo aplica entre archivos distintos.)

## 6. Punto de entrada del programa

```ostrin
// main.ostrin

fn main() -> Void {
    print("hola desde Ostrin")
}
```

El módulo raíz del proyecto (`main.ostrin`, configurable) debe declarar `fn main() -> Void` (o `fn main() -> Result<Void, String>`, para poder propagar un error de arranque hasta la terminal con `try`) como punto de entrada del ejecutable. Este documento no cubre todavía cómo se estructura un proyecto multi-archivo a nivel de configuración/build (nombre del paquete, dependencias externas, versión del compilador) — eso pertenece al futuro "Ostrin Package System" mencionado en la idea original, fuera de alcance aquí.

---

## 7. Preguntas abiertas para la siguiente sesión de diseño

1. ~~Sistema de paquetes~~ — resuelto en el documento 14.
2. **Visibilidad intermedia**: si en la práctica dos niveles (privado/público) resultan insuficientes para proyectos grandes, evaluar un tercer nivel tipo "visible dentro de este paquete mismo" — decisión deliberadamente diferida hasta tener casos reales.
3. ~~`as D`~~ y ~~`dyn Trait`~~ — resueltos en los documentos 16 y 15 respectivamente.
