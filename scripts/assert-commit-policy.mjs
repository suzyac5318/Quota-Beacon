import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

function fail(message) {
  console.error(`Commit policy failed: ${message}`);
  process.exit(1);
}

function argumentValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

const identity = JSON.parse(readFileSync(new URL("../PRODUCT_LINE.json", import.meta.url), "utf8"));
if (identity.productLine !== "macos" || identity.expectedBranch !== "macos") {
  fail("this policy file is only valid for the macOS product line.");
}
if (identity.commitPrefix !== "macos-" || identity.tagPrefix !== "macos-v") {
  fail("PRODUCT_LINE.json must use macos- commits and macos-v tags.");
}

const releasePattern = /^macos-v\d+\.\d+\.\d+: .+/;
const maintenancePattern = /^macos-(feat|fix|docs|test|build|ci|chore|refactor|perf|style): .+/;
const isAllowed = (subject) =>
  releasePattern.test(subject) || maintenancePattern.test(subject) || subject.startsWith("Merge ");

const actor = argumentValue("--actor") ?? "";
if (actor.toLowerCase().startsWith("dependabot")) {
  console.log("Commit policy passed: Dependabot actor uses GitHub-managed commit subjects.");
  process.exit(0);
}

const explicitSubject = argumentValue("--subject");
let subjects;
if (explicitSubject) {
  subjects = [explicitSubject];
} else {
  const base = argumentValue("--base");
  const head = argumentValue("--head") ?? "HEAD";
  const usableBase = base && !/^0+$/.test(base);
  try {
    let range;
    if (!usableBase) {
      range = head;
    } else {
      const policyStart = identity.commitPolicyStart;
      if (!policyStart) {
        fail("PRODUCT_LINE.json is missing commitPolicyStart.");
      }
      git("merge-base", "--is-ancestor", policyStart, head);
      try {
        git("merge-base", "--is-ancestor", policyStart, base);
        range = `${base}..${head}`;
      } catch {
        try {
          git("merge-base", "--is-ancestor", base, policyStart);
          range = `${policyStart}^..${head}`;
        } catch {
          fail("base and head do not share the macOS commit-policy lineage.");
        }
      }
    }
    subjects = git("log", "--format=%s", range).split(/\r?\n/).filter(Boolean);
  } catch (error) {
    fail(`unable to read commit range: ${error.message}`);
  }
}

if (subjects.length === 0) {
  fail("no commits found in the requested range.");
}
const invalid = subjects.filter((subject) => !isAllowed(subject));
if (invalid.length > 0) {
  fail(`invalid subject(s): ${invalid.join(" | ")}`);
}

console.log(`Commit policy passed for ${subjects.length} macOS commit(s).`);
