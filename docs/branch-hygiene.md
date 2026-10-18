# Branch hygiene

Ostrin mantiene `main` como rama remota de referencia. Las ramas locales son
temporales y deben desaparecer después de integrar su trabajo.

Ejecuta:

```text
node scripts/branch-hygiene-check.mjs
```

El informe clasifica cada rama local como:

- `base-local`: copia local de `main`, comparada con `origin/main`; puede estar atrasada.
- `integrated`: todos sus commits ya están en la base indicada; puede eliminarse cuando
  ningún worktree la tenga adjunta.
- `unique-commits`: todavía contiene commits que no están en esa base; debe
  conservarse o integrarse mediante una revisión.

Actualiza `origin/main` con `git fetch origin` antes de auditar el estado remoto.
Cuando existe `origin/main`, el auditor usa esa referencia local como base. El número de
`cached origin refs` incluye referencias locales de PR obtenidas por `git fetch`; para
contar ramas remotas reales, consulta GitHub o `git ls-remote --heads origin`.

Para usarlo como una condición estricta después de limpiar los worktrees:

```text
node scripts/branch-hygiene-check.mjs --strict
```

El auditor no elimina ramas. La eliminación se hace deliberadamente con
`git branch -d` después de comprobar que el worktree asociado ya no se usa.
