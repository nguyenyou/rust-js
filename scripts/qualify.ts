// Qualifies a distribution (ADR 0094): the packages `pack:distribution`
// made are checked against their checksums, installed into a project of
// their own, outside this checkout, and the test suite is run through the
// compiler installed there, as an app runs it. What was run, on what, and
// how it did is written as a report; it's qualified only if all of it
// passed.
//
//   bun scripts/qualify.ts <distribution-dir> <report-dir>

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { cpus, release, tmpdir, totalmem } from "node:os";
import { join, resolve } from "node:path";

import { runSync, stopped } from "../test/child";

const root = join(import.meta.dir, "..");
const installTimeout = 5 * 60_000;
const suiteTimeout = 60 * 60_000;

type Suite = {
  name: string;
  command: string[];
  compiler: string;
  passed: boolean;
  pass: number;
  fail: number;
  skip: number;
  // Each by name, so what "qualified" didn't run is said.
  skipped: string[];
  seconds: number;
  log: string;
};

/** What's wrong with a distribution's files: each is listed in SHA256SUMS
 * with the hash it has, and each artifact is the one distribution.json
 * names. */
export function checksums(dir: string): string[] {
  const problems: string[] = [];
  const sums = new Map<string, string>();
  for (const line of readFileSync(join(dir, "SHA256SUMS"), "utf8").split("\n").filter(Boolean)) {
    const [hash, file] = line.split(/\s+/);
    sums.set(file, hash);
  }
  const manifest = JSON.parse(readFileSync(join(dir, "distribution.json"), "utf8"));
  const files = [...manifest.artifacts.map((a: { file: string }) => a.file), "distribution.json"];
  for (const file of files) {
    const path = join(dir, file);
    if (!existsSync(path)) {
      problems.push(`${file} is missing`);
      continue;
    }
    const hash = createHash("sha256").update(readFileSync(path)).digest("hex");
    if (sums.get(file) !== hash) problems.push(`${file} isn't what SHA256SUMS says`);
    const artifact = manifest.artifacts.find((a: { file: string }) => a.file === file);
    if (artifact && artifact.sha256 !== hash) problems.push(`${file} isn't what distribution.json says`);
  }
  for (const file of sums.keys()) if (!files.includes(file)) problems.push(`SHA256SUMS lists ${file}, which the distribution doesn't`);
  return problems;
}

const text = (cmd: string[], cwd = root) => runSync(cmd, cwd, 60_000).stdout.trim();

function suite(name: string, tests: string[], compiler: string, logs: string): Suite {
  const command = [process.execPath, "test", ...tests];
  const started = Date.now();
  const p = runSync(command, root, suiteTimeout, { RUST_JS_COMPILER: compiler });
  const output = `${p.stdout}${p.stderr}`;
  const log = join(logs, `${name}.log`);
  writeFileSync(log, output);
  const count = (what: string) => Number(new RegExp(String.raw`^ (\d+) ` + what + "$", "m").exec(output)?.[1] ?? 0);
  const [pass, fail, skip] = [count("pass"), count("fail"), count("skip")];
  return {
    name,
    command: ["bun", "test", ...tests],
    compiler,
    passed: p.code === 0 && !stopped(p, suiteTimeout) && pass > 0 && fail === 0,
    pass,
    fail,
    skip,
    skipped: [...new Set([...output.matchAll(/^\(skip\) (.*)$/gm)].map((m) => m[1].trim()))],
    seconds: Math.round((Date.now() - started) / 1000),
    log,
  };
}

async function main() {
  const [distArg, reportArg, ...extra] = process.argv.slice(2);
  if (!distArg || !reportArg || extra.length > 0) throw new Error("Usage: bun scripts/qualify.ts <distribution-dir> <report-dir>");
  const dist = resolve(distArg);
  const reports = resolve(reportArg);
  mkdirSync(reports, { recursive: true });
  const manifest = JSON.parse(readFileSync(join(dist, "distribution.json"), "utf8"));
  const problems = checksums(dist);
  // For this host, from this checkout's commit, unchanged: the tests run
  // are the ones the artifacts were made with.
  if (manifest.platform !== process.platform || manifest.arch !== process.arch) {
    problems.push(`the distribution is for ${manifest.platform}-${manifest.arch}, and this is ${process.platform}-${process.arch}`);
  }
  const commit = text(["git", "rev-parse", "HEAD"]);
  if (manifest.source?.commit !== commit) problems.push(`the distribution is from ${manifest.source?.commit}, and the tests from ${commit}`);
  if (manifest.source?.clean !== true) problems.push("the distribution was made from a checkout with changes of its own");

  // Installed as an app installs it: from the packages alone, offline.
  const project = mkdtempSync(join(tmpdir(), "rust-js-qualify-"));
  const suites: Suite[] = [];
  try {
    const dependency = (file: string) => join(dist, file);
    writeFileSync(
      join(project, "package.json"),
      JSON.stringify({
        private: true,
        type: "module",
        dependencies: {
          "rust-js-build": dependency("rust-js-build.tgz"),
          "vite-plugin-rust-js": dependency("vite-plugin-rust-js.tgz"),
          "rust-js-resources": dependency("resources.tgz"),
          "rust-js-native": dependency("native.tgz"),
        },
        overrides: { "rust-js-build": dependency("rust-js-build.tgz") },
      }),
    );
    // With an empty cache, as a new machine has, so nothing but the
    // packages can be what it installs.
    const install = runSync([process.execPath, "install", "--offline", "--ignore-scripts", "--omit", "peer", "--backend", "copyfile"], project, installTimeout, {
      BUN_INSTALL_CACHE_DIR: join(project, "bun-cache"),
    });
    const launcher = join(project, "node_modules", ".bin", "rust-js");
    const binary = join(project, "node_modules", "rust-js-native", "bin", "compiler");
    if (install.code !== 0 || !existsSync(launcher)) {
      problems.push(`the packages didn't install: ${stopped(install, installTimeout) ?? install.stderr.trim().split("\n").at(-1)}`);
    } else {
      const identity = runSync([launcher, "--version-json"], project, 60_000);
      if (JSON.stringify(JSON.parse(identity.stdout || "null")) !== JSON.stringify(manifest.compiler)) {
        problems.push(`the installed compiler says it's ${identity.stdout.trim() || identity.stderr.trim()}, and the distribution ${JSON.stringify(manifest.compiler)}`);
      } else {
        // Every test file, through the installed launcher; the package
        // test packs a distribution of its own, so it takes the binary the
        // one installed has.
        const files = readdirSync(join(root, "test"))
          .filter((file) => file.endsWith(".test.ts") && file !== "packages.test.ts")
          .sort()
          .map((file) => `test/${file}`);
        suites.push(suite("installed-launcher", files, launcher, reports));
        suites.push(suite("packaged-binary", ["test/packages.test.ts"], binary, reports));
      }
    }
  } finally {
    rmSync(project, { recursive: true, force: true });
  }
  for (const s of suites) if (!s.passed) problems.push(`${s.name}: ${s.fail} failed, ${s.pass} passed; see ${s.log}`);

  const report = {
    qualified: problems.length === 0,
    problems,
    commit,
    distribution: manifest,
    host: { platform: process.platform, arch: process.arch, release: release(), cpu: cpus()[0]?.model ?? "", memory: totalmem() },
    runtimes: { bun: Bun.version, node: text(["node", "--version"]), rustc: text(["rustc", "--version"]) },
    suites,
    // What a distribution doesn't carry yet, so this can't say of it.
    notQualified: ["the WASM compiler, which isn't part of a distribution yet"],
  };
  writeFileSync(join(reports, "qualification.json"), JSON.stringify(report, null, 2) + "\n");
  const lines = [
    `## Qualification of rust-js ${manifest.compiler.version} for ${process.platform}-${process.arch}`,
    "",
    report.qualified ? "**Qualified.**" : "**Not qualified:**",
    ...problems.map((p) => `- ${p}`),
    "",
    "| Suite | Compiler | Passed | Failed | Skipped | Seconds |",
    "|---|---|---|---|---|---|",
    ...suites.map((s) => `| ${s.name} | \`${s.compiler.replace(/^.*node_modules/, "node_modules")}\` | ${s.pass} | ${s.fail} | ${s.skip} | ${s.seconds} |`),
    "",
    `Commit ${commit}; ${report.runtimes.rustc}; Bun ${report.runtimes.bun}; Node ${report.runtimes.node}; ${release()}.`,
    `Not qualified here: ${report.notQualified.join("; ")}.`,
    ...suites.flatMap((s) => s.skipped.map((name) => `Skipped in ${s.name}: ${name}.`)),
  ];
  console.log(lines.join("\n"));
  const summary = process.env.GITHUB_STEP_SUMMARY;
  if (summary) writeFileSync(summary, lines.join("\n") + "\n", { flag: "a" });
  if (!report.qualified) process.exitCode = 1;
}

if (import.meta.main) await main();
