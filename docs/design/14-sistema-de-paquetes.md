# Ostrin — Diseño core: Sistema de Paquetes

Versión: 0.1 (borrador de diseño, previo a implementación)
Depende de: [07-modulos-y-visibilidad.md](07-modulos-y-visibilidad.md)

Decisiones de fondo ya cerradas:
- **Manifiesto en TOML** (`ostrin.toml`), no en sintaxis nativa de Ostrin — separa configuración de código ejecutable y reutiliza un formato ya soportado por herramientas existentes.
- **Descentralizado**: las dependencias se referencian por URL de repositorio, no por un registro central. No hace falta infraestructura de servidor para que exista un ecosistema de librerías Ostrin desde el día uno.

---

## 1. `ostrin.toml`

```toml
[package]
name = "particle_sim"
version = "0.3.0"
entry = "main.ostrin"
ostrin = "^0.1"                    # versión mínima/compatible del compilador de Ostrin

[dependencies]
physics = { git = "github.com/ostrin-lang/physics", tag = "v2.1.0" }
linalg  = { git = "github.com/andres/linalg", rev = "a3f9c2e" }
local_utils = { path = "../shared/utils" }
```

- `[package]`: identidad del proyecto — nombre (para mostrar en mensajes de error/build, no reservado en ningún registro central), versión propia (semver, ver §2), archivo de entrada (documento 07, §6), y la versión de compilador de Ostrin requerida.
- `[dependencies]`: cada entrada asocia un **nombre local** (`physics`, `linalg`, `local_utils` — el que se usará en los `import` del código, documento 07) con una fuente:
  - `git = "<url>", tag = "<tag>"`: un repositorio Git, fijado a un tag (normalmente un tag de versión semver, ver §2).
  - `git = "<url>", rev = "<commit>"`: fijado a un commit exacto — útil durante desarrollo, o cuando el repositorio no publica tags.
  - `path = "<ruta local>"`: una dependencia local (otro directorio del mismo disco, típico en un monorepo o mientras se desarrolla una librería junto al proyecto que la usa).

## 2. Versionado — SemVer

Ostrin adopta [versionado semántico](https://semver.org) estándar (`MAYOR.MENOR.PARCHE`) para la versión propia de un paquete y para interpretar los tags de dependencias Git:

- Cambiar `MAYOR` implica una ruptura de compatibilidad (algo que compilaba contra la versión anterior puede dejar de compilar).
- Cambiar `MENOR` añade funcionalidad de forma compatible hacia atrás.
- Cambiar `PARCHE` es una corrección que no cambia la API pública.

Una dependencia puede fijarse a un tag exacto (`tag = "v2.1.0"`, la forma recomendada para builds reproducibles) o a un rango compatible (`tag = "^2.1.0"`, acepta cualquier tag `2.x.y` con `x.y >= 1.0` presente en el repositorio remoto) — el resolvedor de dependencias (§3) consulta los tags del repositorio remoto para encontrar el más reciente que cumpla el rango.

## 3. Resolución de dependencias y `ostrin.lock`

```bash
ostrinc --project path/to/project
```

- `ostrinc --project DIR` lee `DIR/ostrin.toml`, toma su campo `entry` como punto de entrada
  y sigue resolviendo los imports desde el mismo árbol de proyecto. También acepta la ruta
  directa al manifiesto (`--project DIR/ostrin.toml`).
- La resolución local escribe `ostrin.lock` con las dependencias ordenadas por nombre y rutas
  relativas al manifiesto cuando es posible. Así, clonar el proyecto en otro directorio no cambia
  el lockfile por diferencias de máquina.
- Las dependencias `path` se validan localmente; las dependencias `git` se reconocen pero no se
  descargan de forma implícita. Esto mantiene el compilador sin efectos de red durante una
  compilación normal; el clon debe hacerse explícitamente y luego declararse como `path`.

- `ostrin.lock` se versiona en control de versiones. Con rutas relativas y orden estable, clonar
  el proyecto y compilarlo desde otro directorio conserva el mismo lockfile.

### 3.1 Conflictos de versión (dependencias en diamante)

Si dos dependencias directas requieren, transitivamente, versiones **incompatibles** de un mismo repositorio (una necesita `v1.x` de `math_core`, otra necesita `v2.x`), el resolvedor no intenta hacer convivir ambas versiones en el mismo build en silencio (evita la complejidad de que dos versiones distintas del mismo tipo/trait floten al mismo tiempo, algo que rompería la regla de coherencia del documento 03). En su lugar:

```text
Error OSTRIN-E1120
Version conflict for 'github.com/ostrin-lang/math_core':
    'physics' requires ^1.0 (resolved: v1.4.2)
    'stats' requires ^2.0 (resolved: v2.1.0)
These are incompatible major versions and cannot be unified automatically.
Pin one dependency to a compatible version, or import the other under an
explicit alias to use both versions side by side (see §3.2).
```

### 3.2 Convivencia explícita de dos versiones (caso excepcional)

Cuando de verdad hace falta usar dos versiones incompatibles del mismo paquete a la vez (poco común, pero ocurre en proyectos grandes), se declara explícitamente con nombres locales distintos:

```toml
[dependencies]
math_core_v1 = { git = "github.com/ostrin-lang/math_core", tag = "v1.4.2" }
math_core_v2 = { git = "github.com/ostrin-lang/math_core", tag = "v2.1.0" }
```

Cada una se importa por su nombre local (`import math_core_v1`, `import math_core_v2`) como paquetes completamente independientes desde la perspectiva del compilador — no hay ambigüedad porque nunca comparten un mismo nombre en el código.

## 4. Cómo se importan los módulos de una dependencia

Encaja directamente con el sistema de módulos ya cerrado (documento 07): el nombre local declarado en `[dependencies]` actúa como el segmento raíz del path de import, igual que un directorio del propio proyecto:

```ostrin
import physics.units
import linalg.matrix

d = 5 * units.meter
m = matrix.identity(3)
```

No hay diferencia sintáctica entre importar un módulo propio y uno de una dependencia — el compilador resuelve `physics` contra `ostrin.lock` en vez de contra el propio árbol de archivos del proyecto, pero desde el código es exactamente la misma sintaxis `import`.

## 5. Identidad de paquete sin registro central

Como no existe un registro central donde "reservar" un nombre, la identidad real de una dependencia es su **URL de repositorio**, no el nombre local que cada proyecto le pone (ese nombre es solo un alias local, como ya se vio en §3.2). Dos proyectos pueden llamar `math` a dos librerías completamente distintas sin conflicto, porque lo que identifica a cada una ante el resolvedor es la URL, no el alias. Esto evita el problema clásico de "squatting" de nombres en un registro centralizado — nadie necesita "reclamar" un nombre antes de publicar.

---

## 6. Preguntas abiertas para la siguiente sesión de diseño

1. **Índice/registro de descubrimiento opcional** (no de publicación obligatoria, solo de búsqueda: "¿qué librerías Ostrin existen para X?") — se puede construir después, como una capa encima de este esquema descentralizado, sin cambiar cómo se referencian las dependencias (mismo camino que siguió el ecosistema de Go con sus proxies de módulos).
2. **Verificación de integridad** (hashes de contenido además del commit, para detectar manipulación del historial de un repositorio tras fijar el lock) — relevante para cadena de suministro segura, no diseñado aún.
3. **Workspaces** (varios paquetes Ostrin relacionados en un mismo repositorio, compartiendo un `ostrin.lock`) — útil para proyectos grandes, no cubierto en este documento.
4. Pendientes previos siguen abiertos: `dyn Trait`, `select` sobre canales, elisión de ARC, operadores bit a bit, ordenación general.
