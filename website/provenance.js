const artifact = globalThis.OSTRIN_PROVENANCE;

function setText(selector, value) {
  const node = document.querySelector(selector);
  if (node) node.textContent = value;
}

if (artifact) {
  setText("[data-provenance-schema]", artifact.schema ?? "unknown");
  setText("[data-provenance-source]", artifact.program?.source_hash ?? "unknown");
  setText("[data-provenance-effects]", (artifact.activity?.effects ?? []).join(", ") || "none known");
  setText("[data-provenance-level]", artifact.reproducibility?.level ?? "unverified");
  const json = document.querySelector("[data-provenance-json]");
  if (json) json.textContent = JSON.stringify(artifact, null, 2);
}
