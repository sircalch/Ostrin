import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { collectSiteFacts, renderSiteData, repositoryRoot } from "./site-facts.mjs";

const outputPath = path.join(repositoryRoot, "website", "site-data.js");
const expected = renderSiteData(collectSiteFacts());

if (process.argv.includes("--write")) {
  writeFileSync(outputPath, expected, "utf8");
  console.log(`website-metadata: wrote ${path.relative(repositoryRoot, outputPath)}`);
} else if (!existsSync(outputPath) || readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") !== expected) {
  console.error("website-metadata: website/site-data.js is stale; run node scripts/website-metadata.mjs --write");
  process.exitCode = 1;
} else {
  console.log("website-metadata: ok");
}
