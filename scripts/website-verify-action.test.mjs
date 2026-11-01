import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const actionPath = path.join(repositoryRoot, ".github/actions/website-verify/action.yml");
const pagesWorkflowPath = path.join(repositoryRoot, ".github/workflows/pages.yml");

test("R3 readiness is replayed from the exact uploaded candidate before official evidence publication", () => {
  const action = readFileSync(actionPath, "utf8");
  const prepareEvidence = action.indexOf(
    '--replay-artifact-all . --run-id "$GITHUB_RUN_ID" --tested-sha "$GITHUB_SHA" --write-profile-receipt',
  );
  const candidateUpload = action.indexOf("id: upload-host-candidate");
  const candidateDownload = action.indexOf("uses: actions/download-artifact@v4", candidateUpload);
  const artifactReplay = action.indexOf(
    "--replay-artifact-all target/r3-ci/downloaded-host-evidence --run-id \"$GITHUB_RUN_ID\" --tested-sha \"$GITHUB_SHA\" --require-profile-receipt",
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
  assert.match(action, /target\/r3-ci\/r3-replay-profile-receipt\.json/);
  assert.match(action, /value: \$\{\{ steps\.upload-verified-host-evidence\.outputs\.artifact-url \}\}/);
});

test("pull requests verify the website artifact without deploying Pages", () => {
  const workflow = readFileSync(pagesWorkflowPath, "utf8");
  assert.match(workflow, /pull_request:\s*\n\s+paths:\s*\n\s+- 'website\/\*\*'/);
  assert.match(workflow, /group: pages-\$\{\{ github\.event\.pull_request\.number \|\| github\.ref \}\}/);
  assert.match(workflow, /verify:\s*\n\s+if: github\.event_name == 'pull_request'[\s\S]*?permissions:\s*\n\s+contents: read[\s\S]*?uses: \.\/\.github\/actions\/website-verify/);
  assert.match(workflow, /deploy:\s*\n\s+if: github\.event_name != 'pull_request'[\s\S]*?pages: write[\s\S]*?id-token: write/);
});
