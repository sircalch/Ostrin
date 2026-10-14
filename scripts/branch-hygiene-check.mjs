import { execFileSync } from "node:child_process";

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function lines(value) {
  return value ? value.split(/\r?\n/).filter(Boolean) : [];
}

function count(args) {
  return Number(git(["rev-list", "--count", ...args]));
}

const localBranches = lines(git(["for-each-ref", "--format=%(refname:short)", "refs/heads"]));
const remoteBranches = new Set(
  lines(git(["for-each-ref", "--format=%(refname:strip=3)", "refs/remotes/origin"]))
    .filter((branch) => branch !== "HEAD"),
);
const defaultBranch = remoteBranches.has("main") ? "main" : localBranches.includes("main") ? "main" : null;

if (!defaultBranch) {
  console.error("branch-hygiene-check: could not find origin/main or local main");
  process.exit(1);
}

const report = localBranches.map((branch) => {
  let merged = false;
  try {
    git(["merge-base", "--is-ancestor", branch, defaultBranch]);
    merged = true;
  } catch {
    merged = false;
  }
  return {
    branch,
    merged,
    ahead: count([`${defaultBranch}..${branch}`]),
    behind: count([`${branch}..${defaultBranch}`]),
    remote: remoteBranches.has(branch),
  };
});

const integrated = report.filter((entry) => entry.branch !== defaultBranch && entry.merged);
const unique = report.filter((entry) => entry.branch !== defaultBranch && !entry.merged);

console.log(`branch-hygiene-check: base=${defaultBranch}`);
console.log(`local branches: ${report.length}; origin branches: ${remoteBranches.size}`);
for (const entry of report) {
  const state = entry.branch === defaultBranch ? "base" : entry.merged ? "integrated" : "unique-commits";
  console.log(`${state}: ${entry.branch} ahead=${entry.ahead} behind=${entry.behind} remote=${entry.remote}`);
}
console.log(`integrated local branches eligible for cleanup: ${integrated.length}`);
console.log(`local branches retaining unique commits: ${unique.length}`);

if (process.argv.includes("--strict") && integrated.length > 0) {
  console.error("branch-hygiene-check: integrated local branches remain");
  process.exit(2);
}
