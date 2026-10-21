const status = globalThis.OSTRIN_STATUS;

function el(tag, attrs = {}, children = []) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (key === "text") node.textContent = value;
    else if (key === "className") node.className = value;
    else node.setAttribute(key, value);
  }
  node.append(...children);
  return node;
}

function valueCard(label, value, detail, state = "neutral") {
  return el("article", { className: `status-metric ${state}` }, [
    el("span", { className: "status-metric-label", text: label }),
    el("strong", { className: "status-metric-value", text: value }),
    el("span", { className: "status-metric-detail", text: detail }),
  ]);
}

function statusClass(value) {
  return value === "READY" ? "ready" : value === "OPEN" || value === "NOT_READY" ? "open" : "neutral";
}

function render() {
  if (!status) return;
  const backend = document.querySelector("[data-status-backend]");
  const ownership = document.querySelector("[data-status-ownership]");
  const numeric = document.querySelector("[data-status-numeric]");
  const effects = document.querySelector("[data-status-effects]");
  const reproducibility = document.querySelector("[data-status-reproducibility]");
  const linguist = document.querySelector("[data-status-linguist]");
  const readiness = document.querySelector("[data-status-readiness]");
  const metadata = document.querySelector("[data-status-metadata]");
  backend.replaceChildren(
    valueCard("IR generated", status.nativeBackend.ir.toLocaleString(), "native backend functions", "ready"),
    valueCard("HIR generated", status.nativeBackend.hir.toLocaleString(), "native backend functions", "ready"),
    valueCard("AST fallback", status.nativeBackend.ast.toLocaleString(), "explicitly tracked", status.nativeBackend.ast <= 6 ? "ready" : "open"),
    valueCard("Rust marked tests", status.tests.rustMarkedTests.toLocaleString(), "source-level test attributes", "neutral"),
  );
  ownership.replaceChildren(
    valueCard("Tested", status.ownership.tested.toLocaleString(), "ownership matrix cells", "ready"),
    valueCard("Supported", status.ownership.supported.toLocaleString(), "promoted support cells", "ready"),
    valueCard("Known limits", status.ownership.knownLimitation.toLocaleString(), "documented K cells", "open"),
  );
  if (numeric && status.numericalValidation) {
    const validation = status.numericalValidation;
    numeric.replaceChildren(
      valueCard("Local", validation.local, `${validation.platform ?? "unknown"} report`, validation.local === "verified" ? "ready" : "open"),
      valueCard("Parity", String(validation.comparedValues ?? 0), `max Δ ${validation.maximumAbsoluteDifference ?? "—"}`, validation.local === "verified" ? "ready" : "open"),
      valueCard("FFT references", String(validation.fftCases ?? 0), "radix-2 and direct DFT cases", validation.local === "verified" && validation.fftCases > 0 ? "ready" : "open"),
      valueCard("Leaks", validation.leakCheck, "native executable", validation.local === "verified" && validation.leakCheck === "live_allocations=0" ? "ready" : "open"),
      valueCard("CI attribution", validation.ci === "verified" ? "VERIFIED" : "UNATTRIBUTED", validation.ci === "verified" ? "source-matched report" : "local report has no matching CI provenance", validation.ci === "verified" ? "ready" : "neutral"),
    );
  }
  if (effects && status.effectCoverage) {
    const coverage = status.effectCoverage;
    effects.replaceChildren(
      valueCard("Known operation edges", String(coverage.knownCalls ?? 0), `sum of per-bundle distinct function-operation edges across ${coverage.bundleCount ?? "registered experiment bundles"}`, "ready"),
      valueCard("Symbolic callback edges", String(coverage.symbolicCalls ?? 0), "distinct edges modeled with effect variables", "neutral"),
      valueCard("Unresolved operation edges", String(coverage.unresolvedCalls ?? 0), "distinct conservative boundary edges", coverage.unresolvedCalls === 0 ? "ready" : "open"),
      valueCard("Inventory", coverage.complete ? "COMPLETE" : "PARTIAL", "not a purity proof", coverage.complete ? "ready" : "open"),
    );
  }
  reproducibility.replaceChildren(
    valueCard("R0", String(status.reproducibility.R0), "recorded replay", "neutral"),
    valueCard("R1", String(status.reproducibility.R1), "seeded replay", status.reproducibility.R1 > 0 ? "ready" : "open"),
    valueCard("R2", String(status.reproducibility.R2), "input snapshot", status.reproducibility.R2 > 0 ? "ready" : "open"),
    valueCard("R3 claims", String(status.reproducibility.R3), `${status.reproducibility.R3} fixtures claim R3 · ${status.r3.readyCount}/${status.r3.bundleCount} pass the structural CI capture gate · ${status.r3.closedEntryEffectBundleCount}/${status.r3.bundleCount} entry rows closed · ${status.r3.symbolicEffectCallCount} module-wide symbolic · ${status.r3.unresolvedEffectCallCount} unresolved`, "neutral"),
    valueCard("Source input scan", `${status.r3.sourceReadAudit.literalReadPathCallCount} literal reads`, `${status.r3.sourceReadAudit.bundlesWithDirectReadFileCalls}/${status.r3.sourceReadAudit.sourceBundleCount} bundles · ${status.r3.sourceReadAudit.dynamicReadCallCount} dynamic · ${status.r3.sourceReadAudit.readFileReferenceCount} references · ${status.r3.sourceReadAudit.undeclaredLiteralPathCount} unlisted · ${status.r3.sourceReadAudit.declaredSnapshotWithoutDirectReadCount + status.r3.sourceReadAudit.filesystemCapabilitiesWithoutDirectReadCount} snapshot/capability gaps · static scan only`, "neutral"),
  );
  if (linguist) {
    const indexed = status.linguist.indexedPublicFiles == null ? "—" : status.linguist.indexedPublicFiles.toLocaleString();
    linguist.replaceChildren(
      valueCard("Public .ostrin files", indexed, `threshold ${status.linguist.threshold.toLocaleString()}`, status.linguist.indexedPublicFiles >= status.linguist.threshold ? "ready" : "open"),
      valueCard("Local preparation", status.linguist.preparation, "proposal, grammar and samples", "ready"),
      valueCard("Upstream PR", status.linguist.readyForUpstreamPullRequest ? "READY" : "PENDING", status.linguist.usage, status.linguist.readyForUpstreamPullRequest ? "ready" : "open"),
    );
  }
  const criteria = Object.entries(status.releaseReadiness.criteria).map(([id, value]) => el("li", { className: `status-criterion ${statusClass(value)}` }, [
    el("span", { className: "status-criterion-name", text: id }),
    el("span", { className: "status-criterion-state", text: value }),
  ]));
  readiness.replaceChildren(
    el("div", { className: `status-release ${statusClass(status.releaseReadiness.status)}` }, [
      el("span", { className: "status-release-label", text: `v${status.releaseReadiness.release}` }),
      el("strong", { text: status.releaseReadiness.status }),
    ]),
    el("ul", { className: "status-criteria" }, criteria),
  );
  const rcRevision = status.sourceRevision?.slice(0, 12) ?? "unknown";
  const releaseRevision = status.finalReleaseSourceRevision?.slice(0, 12);
  metadata.replaceChildren(
    el("span", { text: "Dashboard metrics: repository evidence" }),
    el("span", { text: `v0.2 RC binary source: ${rcRevision}` }),
    ...(releaseRevision ? [el("span", { text: `Published v0.2 binary source: ${releaseRevision}` })] : []),
    ...(status.developmentEvidence ? [
      el("a", {
        href: status.developmentEvidence.runUrl,
        text: `Development evidence: ${status.developmentEvidence.workflow} run ${status.developmentEvidence.runId} · tested ${status.developmentEvidence.testedSha.slice(0, 12)}`,
      }),
      ...(status.developmentEvidence.artifactUrl ? [el("a", { href: status.developmentEvidence.artifactUrl, text: `Download ${status.developmentEvidence.artifactName}` })] : [el("span", { text: `Artifact: ${status.developmentEvidence.artifactName} (open the run to download)` })]),
    ] : [el("span", { text: "Development CI capture is not attached to this local scorecard." })]),
    el("span", { text: status.workingTreeDirty ? "repository source changed at generation" : "repository source clean at generation" }),
  );
}

render();
