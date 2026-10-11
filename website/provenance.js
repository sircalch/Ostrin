const artifact = globalThis.OSTRIN_PROVENANCE;

function setText(selector, value) {
  const node = document.querySelector(selector);
  if (node) node.textContent = value;
}

if (artifact) {
  setText("[data-provenance-schema]", artifact.schema ?? "unknown");
  setText("[data-provenance-source]", artifact.program?.source_hash ?? "unknown");
  setText("[data-provenance-effects]", (artifact.activity?.effects ?? []).join(", ") || "none known");
  const noKnownEffects = artifact.activity?.functions_without_known_effects ?? [];
  setText("[data-provenance-no-known-effects]", `${noKnownEffects.length} candidates`);
  const runtime = artifact.runtime;
  const runtimeLabel = runtime?.target
    ? `${runtime.target}${runtime.wasi ? ` · WASI ${runtime.wasi}` : ""} · ${runtime.status ?? "unknown"}`
    : "unknown";
  setText("[data-provenance-runtime]", runtimeLabel);
  setText("[data-provenance-level]", artifact.reproducibility?.level ?? "unverified");
  const json = document.querySelector("[data-provenance-json]");
  if (json) json.textContent = JSON.stringify(artifact, null, 2);
}
