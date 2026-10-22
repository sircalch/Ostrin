import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const actionPath = path.join(repositoryRoot, ".github/actions/website-verify/action.yml");

test("R3 readiness is replayed from the exact uploaded candidate before official evidence publication", () => {
  const action = readFileSync(actionPath, "utf8");
  const prepareEvidence = action.indexOf(
    '--replay-artifact-all . --run-id "$GITHUB_RUN_ID" --tested-sha "$GITHUB_SHA"',
  );
  const candidateUpload = action.indexOf("id: upload-host-candidate");
  const candidateDownload = action.indexOf("uses: actions/download-artifact@v4", candidateUpload);
  const artifactReplay = action.indexOf(
    "--replay-artifact-all target/r3-ci/downloaded-host-evidence",
    candidateDownload,
  );
  const verifiedUpload = action.indexOf("id: upload-verified-host-evidence", artifactReplay);

  assert.ok(prepareEvidence >= 0, "run-bound replay reports must be generated before packaging the candidate");
  assert.ok(candidateUpload >= 0, "candidate evidence upload is missing");
  assert.ok(candidateUpload > prepareEvidence, "candidate evidence must include the run-bound replay reports");
  assert.ok(candidateDownload > candidateUpload, "candidate artifact must be downloaded after upload");
  assert.ok(artifactReplay > candidateDownload, "the downloaded artifact must be independently replayed");
  assert.ok(verifiedUpload > artifactReplay, "official evidence is published only after replay succeeds");
  assert.match(action, /name: ostrin-r3-host-candidate-\$\{\{ github\.run_id \}\}-attempt-\$\{\{ github\.run_attempt \}\}/);
  assert.match(action, /name: ostrin-r3-host-evidence-\$\{\{ github\.run_id \}\}-attempt-\$\{\{ github\.run_attempt \}\}/);
  assert.match(action, /--run-id "\$GITHUB_RUN_ID" --tested-sha "\$GITHUB_SHA"/);
  assert.match(action, /path: target\/r3-ci\/downloaded-host-evidence/);
  assert.match(action, /value: \$\{\{ steps\.upload-verified-host-evidence\.outputs\.artifact-url \}\}/);
});
