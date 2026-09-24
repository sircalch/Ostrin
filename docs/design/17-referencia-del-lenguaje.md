# Ostrin — Referencia del Lenguaje (consolidada)

Versión: 0.1 — primera consolidación, tras cerrar el núcleo del lenguaje (documentos 01–16).

Este documento no toma decisiones de diseño nuevas — es la **fuente única de verdad** para tres cosas que hasta ahora vivían repartidas en 16 documentos distintos (y que ya generaron dos colisiones detectadas en revisiones anteriores, documento 10 §5.2 y §6.1): la gramática formal, las palabras reservadas, y el registro de códigos de error. Cualquier documento de diseño futuro debe actualizar **este** archivo, no limitarse a introducir su propia tabla local.

De aquí en adelante, los documentos 01–16 siguen siendo la referencia de **por qué** se decidió cada cosa (el razonamiento, las alternativas consideradas, los ejemplos); este documento es la referencia de **qué es exactamente válido escribir**.

---

## 1. Léxico

### 1.1 Comentarios

Nunca se habían formalizado pese a usarse en todos los ejemplos desde el documento 01:

```ostrin
// comentario de una línea, hasta el final de la línea

/* comentario
   de varias líneas */
```

### 1.2 Identificadores

```text
IDENT := (letra | '_') (letra | dígito | '_')*
```

Convención (no impuesta por el compilador, pero sí por `derive`/mensajes de error que la asumen): `snake_case` para variables/funciones/módulos, `PascalCase` para tipos/traits/constructores de variante (documento 05, corrección inicial).

### 1.3 Literales numéricos

```text
INT_LITERAL   := dígito ('_'? dígito)*
FLOAT_LITERAL := dígito ('_'? dígito)* ('.' dígito ('_'? dígito)*)? EXPONENT?   (al menos '.' o EXPONENT)
EXPONENT      := ('e' | 'E') ('+' | '-')? dígito+
```

El `_` opcional como separador visual de miles (`1_000_000`) no cambia el valor — es puramente legibilidad, común en código científico con números grandes.

### 1.4 Otros literales

```text
BOOL_LITERAL   := 'true' | 'false'
CHAR_LITERAL   := "'" carácter_unicode_o_escape "'"
STRING_LITERAL := '"' (carácter_unicode_o_escape)* '"'
```

Escapes estándar dentro de `CHAR_LITERAL`/`STRING_LITERAL`: `\n`, `\t`, `\\`, `\"`, `\'`, `\u{XXXX}` (Unicode por código de punto).

### 1.5 Literal con unidad (documento 01, §3.2)

```text
UNIT_LITERAL := (INT_LITERAL | FLOAT_LITERAL) WS unit_expr
unit_expr    := unit_atom (('*' | '/') unit_atom)*
unit_atom    := IDENT ('^' '-'? INT_LITERAL)?
```

`WS` (al menos un espacio) es obligatorio entre el número y la unidad. Todo `unit_expr` va en la
misma línea, y tras `*` o `/` solo se absorbe un **símbolo de unidad conocido**: `8 m / t` divide
por la variable `t`. Los literales científicos (`6.022e23`, `1e-9`, `532e-9 m`) son `Float`
(añadidos el 2026-09-24).

`as` admite la misma forma a la derecha: `v as km/h`, `g as m/s^2`. La conversión exige la misma
dimensión (E1026). Catálogo de unidades y dimensiones derivadas con nombre: documento 01, §3.6.

---

## 2. Palabras reservadas

```text
fn  mut  return  if  else  match  for  in  while  loop  break  continue
trait  impl  record  enum  pub  import  as  self  Self  dyn
and  or  not  within  to  until  step  approximately  tolerance
try  catch  spawn  spawn_scope  channel
unit  dimension  define
true  false
```

Ninguna se puede usar como nombre de variable, función, campo, parámetro o módulo. (Corrección aplicada en el documento 10, §6.1: un ejemplo anterior usaba `unit` como nombre de parámetro por error.)

### 1.6 Terminación de sentencias — saltos de línea significativos

Ostrin no usa `;`. Un salto de línea separa sentencias, **excepto** cuando el token que sigue solo puede ser continuación de la expresión anterior. La ambigüedad real es acotada: de todos los tokens que pueden empezar una expresión, solo tres **también** pueden continuar una expresión previa como operador binario/postfijo:

```text
-   (resta binaria, o negación unaria de una sentencia nueva)
(   (llamada a función, o una expresión entre paréntesis nueva)
[   (indexado, o un literal de List/Map nuevo)
```

Regla: si el token es uno de estos tres **y** viene precedido por un salto de línea **y** no estamos ya dentro de un `(...)`/`[...]` abierto, se trata como el inicio de una sentencia nueva, no como continuación:

```ostrin
total = a
    + b          // continúa: '+' nunca es ambiguo, no hace falta que esté en la misma línea

x = 7
-x               // sentencia nueva: 'x = 7' termina, '-x' es otra sentencia

result = items
    .filter(is_valid)
    .map(square) // continúa: '.' nunca es ambiguo, el encadenamiento de métodos cruza líneas sin problema
```

Ningún otro operador (`+`, `*`, `/`, `==`, `and`, `or`, `to`, `until`, `as`, `within`, `.`) necesita esta regla: ninguno de ellos puede aparecer como el primer token de una sentencia nueva, así que un salto de línea antes de ellos nunca es ambiguo y siempre se trata como continuación. Dentro de un `(...)` o `[...]` ya abierto (argumentos de función, agrupación, listas/mapas), los saltos de línea nunca terminan nada — la regla de arriba solo aplica a nivel de sentencia.

*(Decisión añadida tras implementar el parser (documento 10 §6, primera versión del compilador): ningún documento de diseño anterior había cerrado esta regla explícitamente pese a que todos los ejemplos ya asumían saltos de línea como separador.)*

---

## 3. Gramática (EBNF simplificado)

Notación: `[...]` opcional, `{...}` repetición cero o más veces, `|` alternativa. Se omiten detalles de asociatividad de operadores (cubiertos en la tabla de precedencia, §4).

### 3.1 Programa y módulos

```ebnf
program      := { top_level_decl }
top_level_decl := [ "pub" ] ( fn_decl | record_decl | enum_decl | trait_decl | impl_decl
                             | unit_decl | dimension_decl | define_decl | import_decl )

import_decl  := "import" module_path [ "as" IDENT ]
              | "import" module_path "." "{" IDENT { "," IDENT } "}"
              | "pub" "import" module_path "." "{" IDENT { "," IDENT } "}"
module_path  := IDENT { "." IDENT }
```

### 3.2 Declaraciones de tipo

```ebnf
record_decl  := "record" IDENT [ generic_params ] [ ":" trait_list ] "{" { field } "}"
field        := [ "pub" ] [ "mut" ] IDENT ":" type [ "=" expr ]

enum_decl    := "enum" IDENT [ generic_params ] [ ":" trait_list ] "{" { variant } "}"
variant      := IDENT [ "(" variant_field { "," variant_field } ")" ]
variant_field:= [ IDENT ":" ] type

trait_decl   := "trait" IDENT [ generic_params ] [ ":" trait_list ] "{" { trait_member } "}"
trait_member := fn_sig ( ";" | block )
trait_list   := IDENT { "+" IDENT }

impl_decl    := "impl" [ generic_params ] IDENT [ "<" type_args ">" ] "for" type "{" { fn_decl } "}"

unit_decl    := "unit" IDENT ":" dimension_expr
dimension_decl := "dimension" IDENT
define_decl  := "define" UNIT_LITERAL "=" UNIT_LITERAL
dimension_expr := IDENT { ( "*" | "/" ) IDENT } | IDENT "^" INT_LITERAL
```

### 3.3 Funciones

```ebnf
fn_decl      := "fn" IDENT [ generic_params ] "(" [ param_list ] ")" "->" type block
fn_sig       := "fn" IDENT [ generic_params ] "(" [ param_list ] ")" "->" type
generic_params := "<" generic_param { "," generic_param } ">"
generic_param  := IDENT [ ":" bound_list ]
bound_list     := bound { "+" bound }
bound          := IDENT | IDENT ":" "Dimension"

param_list   := self_param [ "," param { "," param } ] | param { "," param }
self_param   := "self" | "mut" "self"
param        := IDENT ":" type [ "=" expr ]

lambda       := "fn" "(" [ param_list_no_self ] ")" block
param_list_no_self := IDENT { "," IDENT }   (* tipos inferidos del contexto de uso *)
```

### 3.4 Tipos

```ebnf
type         := simple_type | fn_type | "dyn" trait_list
simple_type  := IDENT [ "<" type_args ">" ]
fn_type      := "fn" "(" [ type { "," type } ] ")" "->" type
type_args    := type_or_dim { "," type_or_dim }
type_or_dim  := type | dimension_expr
```

### 3.5 Sentencias y bloques

```ebnf
block        := "{" { statement } [ expr ] "}"
statement    := binding | assignment | expr_statement | "return" [ expr ]
              | "break" [ expr ] | "continue"

binding      := [ "mut" ] IDENT [ ":" type ] "=" expr
assignment   := IDENT "=" expr                      (* solo válido si IDENT es 'mut' *)
expr_statement := expr
```

### 3.6 Expresiones (de menor a mayor nivel; ver precedencia en §4)

```ebnf
expr         := if_expr | match_expr | loop_expr | binary_expr | trailing_call

if_expr      := "if" expr block "else" block
              | "if" expr block                      (* como sentencia, sin valor *)
if_let       := "if" "let" pattern "=" expr block
match_expr   := "match" expr "{" match_arm { "," match_arm } [ "," ] "}"
match_arm    := pattern [ "if" expr ] "=>" ( expr | block )

loop_expr    := "loop" block
for_stmt     := "for" pattern "in" expr block   (* 'in' es palabra reservada, ver §2 *)
while_stmt   := "while" expr block

try_expr     := "try" expr [ "catch" lambda ]

trailing_call := IDENT "(" [ arg_list ] ")" [ block ]
generic_call  := IDENT "<" type_args ">" "(" [ arg_list ] ")" [ block ]
record_literal := IDENT "{" field_init { "," field_init } "}"
generic_record_literal := IDENT "<" type_args ">" "{" field_init { "," field_init } "}"
field_init   := IDENT ":" expr
arg_list     := arg { "," arg }
arg          := expr | IDENT ":" expr

binary_expr  := (* ver tabla de precedencia, §4 *)
range_expr   := expr ( "to" | "until" ) expr [ "step" expr ]
```

En el prototipo, `generic_call` también puede tener como callee un acceso a
método (`container.map<U>(value)`); la forma abreviada con `IDENT` mantiene la
gramática EBNF legible sin repetir toda la producción de expresiones postfix.

### 3.7 Patrones (`match`, `if let`, `for`)

```ebnf
pattern      := literal | range_pattern | "_" | IDENT
              | variant_pattern | tuple_pattern
variant_pattern := IDENT [ "(" pattern_field { "," pattern_field } ")" ]
pattern_field   := IDENT [ ":" pattern ] | pattern
tuple_pattern   := "(" pattern { "," pattern } ")"
range_pattern   := literal ( "to" | "until" ) literal
```

### 3.8 Literales de colección

```ebnf
list_literal := "[" [ expr { "," expr } ] "]"
map_literal  := "[" [ (expr ":" expr) { "," expr ":" expr } ] "]"
set_literal  := "{" expr { "," expr } "}"
```

### 3.9 Concurrencia

```ebnf
spawn_expr       := "spawn" block
spawn_scope_expr := "spawn_scope" block
channel_expr     := "channel" "<" type ">" "(" [ "capacity" ":" expr ] ")"
```

---

## 4. Precedencia de operadores (canónica — reemplaza cualquier copia local)

De mayor a menor precedencia (documento 08, §6, ya corregida tras la segunda revisión de consistencia):

```text
1.  llamada a función, acceso a campo (.), indexado ([])
2.  ^ (exponente de unidad, dentro de una expresión de unidad)
3.  unario: - (negación numérica), not
4.  * /
5.  + -
6.  as
7.  to, until, step
8.  == != < > <= >=
9.  within, approximately ... tolerance ...
10. and
11. or
12. = (asignación — solo como sentencia)
```

---

## 5. Registro de códigos de error/advertencia (canónico — reemplaza cualquier copia local)

| Código | Origen | Significado |
|---|---|---|
| E1001 | 01 | Reasignar un binding inmutable |
| E1002 | 01 | Asignar a un campo inmutable o a través de un binding inmutable |
| E1010 | 01 | Unidad desconocida |
| E1024 | 01 | Operación dimensional inválida (sumar dimensiones distintas) |
| E1025 | 01 | Falta unidad explícita al combinar escalar con `Quantity` |
| E1040 | 02 | Sobrecarga de función no soportada |
| E1041 | 01 / 02 | Tipo de retorno, binding, campo o asignación incompatible |
| E1042 | 02 / 03 | Argumento genérico incompatible o que incumple un trait bound |
| E1043 | 01 | Acceso a un campo inexistente |
| E1050 | 12 | `impl` de un trait sin su supertrait implementado |
| E1051 | 12 | `derive` sobre un tipo con un campo que no cumple el trait |
| E1052 | 12 | `derive` y `impl` manual compitiendo por el mismo trait |
| E1053 | 03 / 13 | Llamar un método `mut self` sobre un binding inmutable |
| E1054 | 03 | Trait desconocido, `impl` duplicado o firma de método incompatible |
| E1055 | 03 | Método requerido de trait no implementado |
| E1056 | 03 | `impl` que viola la regla de coherencia/orphan |
| E1057 | 03 | Ciclo o declaraciones incompatibles en la herencia de traits |
| E1060 | 05 | `match` no exhaustivo |
| E1061 | 05 | Forma, campo o tipo de patrón inválido |
| E1080 | 07 | Acceso a símbolo privado desde otro módulo |
| E1081 | 07 | Ciclo de importación |
| E1090 | 08 | `approximately` sin `tolerance` |
| E1100 | 09 | Captura de `mut` en `spawn` |
| E1101 | 09 | Uso de un valor `mut` después de moverlo a un canal |
| E1120 | 14 | Conflicto de versión de dependencias |
| E1130 | 15 | Trait no compatible con `dyn` |
| E1041 | implementación | Tipo de retorno declarado no coincide con el del cuerpo (u otro descarte de tipos general) — introducido al implementar el verificador de tipos, documento 10 §7 |
| E1091 | implementación | Dimensiones distintas entre los operandos de `approximately`/`tolerance` — complementa E1090 (falta `tolerance`), introducido al implementar el verificador de tipos |
| W2001 | 04 | `Result`/`Option` no consumido (advertencia) |

**Convención de numeración para códigos nuevos**: rango `E10xx` para variables/tipos/unidades/funciones (docs 01-02), `E105x` para traits/derive (docs 03/12), `E106x` para enum/pattern matching (doc 05), `E108x` para módulos (doc 07), `E109x` para lógica/expresiones (doc 08), `E11xx` para concurrencia (doc 09), `E112x` para paquetes (doc 14), `E113x` para `dyn Trait` (doc 15). Antes de asignar un código nuevo, comprobar en esta tabla que no esté en uso — es exactamente el paso que faltó y causó la colisión de `E1052` detectada en el documento 10, §5.2.

---

## 6. Estado de esta referencia

Generada a partir de una lectura completa de los documentos 01–16. No es (todavía) una gramática formal verificada con un generador de parsers — es la primera consolidación manual, y sirve como punto de partida para quien implemente el lexer/parser. Cualquier ambigüedad que aparezca al formalizarla en una herramienta real (ej. conflictos de precedencia no anticipados, casos límite de la gramática de patrones) debe resolverse actualizando **este** documento, y solo después, si cambia una decisión de diseño, el documento 01–16 correspondiente.
