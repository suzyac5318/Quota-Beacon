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
if (identity.productLine !== "windows" || identity.expectedBranch !== "Windows") {
  fail("this policy file is only valid for the Windows product line.");
}
if (identity.commitPrefix !== "windows-" || identity.tagPrefix !== "windows-v") {
  fail("PRODUCT_LINE.json must use windows- commits and windows-v tags.");
}

const releasePattern = /^windows-v\d+\.\d+\.\d+: .+/;
const maintenancePattern = /^windows-(feat|fix|docs|test|build|ci|chore|refactor|perf|style): .+/;
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
    subjects = usableBase
      ? git("log", "--format=%s", `${base}..${head}`).split(/\r?\n/).filter(Boolean)
      : [git("log", "-1", "--format=%s", head)];
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

console.log(`Commit policy passed for ${subjects.length} Windows commit(s).`);
