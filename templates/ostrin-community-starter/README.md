# Ostrin community starter

Use this small project as a starting point for a public Ostrin experiment,
example, benchmark or scientific visualization. Keeping projects public and
reproducible helps the language mature and creates independent `.ostrin`
examples for a future GitHub Linguist proposal.

## Quick start

From the Ostrin repository checkout:

```powershell
cargo run --manifest-path compiler/Cargo.toml -- --run --project templates/ostrin-community-starter
```

Expected output:

```text
Hello from a public Ostrin project
```

## Publish a useful example

1. Copy this directory into a new public repository.
2. Add a short README describing the question, inputs, command and expected output.
3. Keep generated figures or tables accompanied by the source that creates them.
4. Commit the generated `ostrin.lock` and record the Ostrin commit or release used to run the example.
5. Link the project from an issue or discussion in [Ostrin](https://github.com/sircalch/Ostrin).

Do not claim that GitHub recognizes `.ostrin` yet. The current adoption status
and the evidence required for Linguist are documented in
[`docs/linguist.md`](https://github.com/sircalch/Ostrin/blob/main/docs/linguist.md).
