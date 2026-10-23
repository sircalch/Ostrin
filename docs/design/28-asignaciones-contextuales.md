# Asignaciones contextuales y tipos estables

Las asignaciones a un binding `mut` reutilizan el tipo del slot que ya existe. El
lado derecho se verifica con ese tipo como contexto antes de actualizar el entorno
del checker.

Esto evita que una reasignación vuelva a inferir un constructor algebraico con
parámetros desconocidos. Por ejemplo:

```ostrin
mut result = Ok("value")
result = Err("message")
```

La primera expresión produce `Result<String, ?>`. La segunda conserva `String`
como payload de éxito, completa el error como `String` y deja el binding como
`Result<String, String>`. El mismo mecanismo conserva el tipo de `Option<T>` al
asignar `None` y aplica a records genéricos.

Las reglas son:

1. un binding inmutable conserva su diagnóstico `E1001`;
2. un binding mutable recibe el tipo previo como esperado;
3. los literales numéricos y listas se adaptan después de la inferencia contextual;
4. una asignación incompatible produce `E1041` y no cambia el tipo del slot;
5. los parámetros `Unknown` sólo se refinan cuando la nueva expresión aporta
   evidencia compatible.

La referencia semántica sigue siendo el intérprete. La prueba nativa de regresión
compara su salida, fuerza la ruta IR y exige `live_allocations=0`; el reporte de
tipo y la emisión WASI se usan para comprobar que el tipo refinado no cae en un
fallback opaco.
