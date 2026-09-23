# Ostrin tests

The current integration suite lives in
[`compiler/tests/examples.rs`](../compiler/tests/examples.rs) because it runs
against the Rust compiler binary through Cargo.

This root directory is reserved for language-level fixtures and future tests
that are independent of the compiler implementation.

## Browser regression tests

`browser/` contains Chromium end-to-end checks for the GitHub Pages subpath, the
responsive public pages, mobile navigation, and the real WASM compiler's output
and diagnostics. From `tests/browser/`, install the pinned Node dependencies and
browser once, then run:

```sh
npm ci
npx playwright install chromium
npm test
```

`npm test` builds `wasm32-wasip1` from the current compiler sources and places the
ignored artifact in `website/ostrinc.wasm` before launching the local server. A
Rust toolchain with the `wasm32-wasip1` target is required. CI and the Pages
deployment call the same verification action, so the published playground is
smoke-tested before upload.
