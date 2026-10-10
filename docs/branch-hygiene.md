# Branch hygiene

Ostrin mantiene `main` como rama remota de referencia. Las ramas locales son
temporales y deben desaparecer después de integrar su trabajo.

Ejecuta:

```text
node scripts/branch-hygiene-check.mjs
```

El informe clasifica cada rama local como:

- `integrated`: todos sus commits ya están en `main`; puede eliminarse cuando
  ningún worktree la tenga adjunta.
- `unique-commits`: todavía contiene commits que no están en `main`; debe
  conservarse o integrarse mediante una revisión.

Para usarlo como una condición estricta después de limpiar los worktrees:

```text
node scripts/branch-hygiene-check.mjs --strict
```

El auditor no elimina ramas. La eliminación se hace deliberadamente con
`git branch -d` después de comprobar que el worktree asociado ya no se usa.
