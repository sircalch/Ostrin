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
  const unresolvedCalls = artifact.activity?.unresolved_calls ?? [];
  const unresolvedCount = unresolvedCalls.reduce(
    (total, entry) => total + (Array.isArray(entry.calls) ? entry.calls.length : 0),
    0,
  );
  setText("[data-provenance-unresolved-calls]", `${unresolvedCount} calls`);
  const unresolvedList = document.querySelector("[data-provenance-unresolved-list]");
  if (unresolvedList) {
    unresolvedList.replaceChildren();
    if (unresolvedCalls.length === 0) {
      const item = document.createElement("li");
      item.textContent = "No unresolved calls recorded.";
      unresolvedList.append(item);
    } else {
      for (const entry of unresolvedCalls) {
        const item = document.createElement("li");
        item.textContent = `${entry.function}: ${(entry.calls ?? []).join(", ")}`;
        unresolvedList.append(item);
      }
    }
  }
  const runtime = artifact.runtime;
  const runtimeLabel = runtime?.target
    ? `${runtime.target}${runtime.wasi ? ` · WASI ${runtime.wasi}` : ""} · ${runtime.status ?? "unknown"}`
    : "unknown";
  setText("[data-provenance-runtime]", runtimeLabel);
  setText("[data-provenance-level]", artifact.reproducibility?.level ?? "unverified");
  const json = document.querySelector("[data-provenance-json]");
  if (json) json.textContent = JSON.stringify(artifact, null, 2);
}
