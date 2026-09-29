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
import { join, relative, resolve } from "node:path";

import { runSync, stopped } from "../test/child";

const root = join(import.meta.dir, "..");
// Settings that would change what the suite checks, left out of every run:
// `BLESS` rewrites what snapshots expect, `RUST_JS_SNAPSHOTS` skips the
// corpus's, the others choose tests or how long they have. The suite runs
// as it would with none of them.
const unset = ["BLESS", "RUST_JS_SNAPSHOTS", "FUZZ_START", "FUZZ_SEEDS", "FUZZ_REDUCE_BUDGET", "FUZZ_REPORT", "RUST_JS_COMPILE_TIMEOUT", "RUST_JS_REQUIRE_WASM", "RUST_JS_DISTRIBUTION"];
const cleared = Object.fromEntries(unset.map((name) => [name, undefined]));
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

// The distribution's packages, by the tarball each is installed from.
const packages: [string, string][] = [
  ["rust-js-build", "rust-js-build.tgz"],
  ["vite-plugin-rust-js", "vite-plugin-rust-js.tgz"],
  ["rust-js-resources", "resources.tgz"],
  ["rust-js-native", "native.tgz"],
  ["@rust-js/runtime", "runtime.tgz"],
];

/** Every file under `dir`, by its path from there. */
function files(dir: string): string[] {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(dir, join(entry.parentPath, entry.name)))
    .sort();
}

const hash = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");

/** A Vite app, outside the checkout, built with the distribution's plugin,
 * tooling, resources and compiler, from its tarballs alone, each installed
 * file the tarball's, and Vite and React from the registry, at the versions
 * this checkout locks. */
export function viteApp(dist: string, logs: string): Suite {
  const started = Date.now();
  const app = mkdtempSync(join(tmpdir(), "rust-js-qualify-app-"));
  const problems: string[] = [];
  const log: string[] = [];
  try {
    const locked = (name: string) => JSON.parse(readFileSync(join(root, "node_modules", name, "package.json"), "utf8")).version as string;
    const dependencies: Record<string, string> = Object.fromEntries(packages.map(([name, file]) => [name, join(dist, file)]));
    for (const name of ["vite", "@vitejs/plugin-react", "react", "react-dom"]) dependencies[name] = locked(name);
    writeFileSync(join(app, "package.json"), JSON.stringify({ private: true, type: "module", dependencies, overrides: { "rust-js-build": join(dist, "rust-js-build.tgz") } }));
    writeFileSync(
      join(app, "vite.config.js"),
      'import { defineConfig } from "vite";\nimport react from "@vitejs/plugin-react";\nimport rustJs from "vite-plugin-rust-js";\n\nexport default defineConfig({ plugins: [rustJs({ crates: ["src/App.rs"], bindings: ["react"] }), react()] });\n',
    );
    writeFileSync(join(app, "index.html"), '<div id="root"></div><script type="module" src="/src/main.jsx"></script>\n');
    mkdirSync(join(app, "src"));
    writeFileSync(join(app, "src", "main.jsx"), 'import { createRoot } from "react-dom/client";\nimport { App } from "./App.jsx";\n\ncreateRoot(document.getElementById("root")).render(<App />);\n');
    writeFileSync(join(app, "src", "App.rs"), '#![allow(non_snake_case)]\nuse react::Element;\n\npub fn App() -> Element {\n    jsx! { <main><h1>{"Qualified"}</h1></main> }\n}\n');
    const install = runSync([process.execPath, "install", "--ignore-scripts"], app, installTimeout, { ...cleared, BUN_INSTALL_CACHE_DIR: join(app, "bun-cache") });
    log.push(install.stdout, install.stderr);
    if (install.code !== 0) problems.push(`the app didn't install: ${stopped(install, installTimeout) ?? install.stderr.trim().split("\n").at(-1)}`);
    else {
      // Each rust-js package the app has is its tarball, file for file.
      for (const [name, file] of packages) {
        const unpacked = mkdtempSync(join(tmpdir(), "rust-js-qualify-package-"));
        try {
          runSync(["tar", "-xzf", join(dist, file), "-C", unpacked], app, 60_000);
          const expected = join(unpacked, "package"), installed = join(app, "node_modules", name);
          const want = files(expected);
          const differ = want.filter((path) => !existsSync(join(installed, path)) || hash(join(installed, path)) !== hash(join(expected, path)));
          if (want.length === 0 || differ.length > 0) problems.push(`the app's ${name} isn't ${file}: ${differ.slice(0, 3).join(", ") || "it's empty"}`);
        } finally {
          rmSync(unpacked, { recursive: true, force: true });
        }
      }
      const build = runSync(["node", join(app, "node_modules", "vite", "bin", "vite.js"), "build"], app, suiteTimeout);
      log.push(build.stdout, build.stderr);
      const why = () => build.stderr.split("\n").find((line) => /error/i.test(line))?.trim() ?? build.stderr.trim().split("\n").at(-1);
      if (build.code !== 0 || stopped(build, suiteTimeout)) problems.push(`the app didn't build: ${stopped(build, suiteTimeout) ?? why()}`);
      else {
        const assets = existsSync(join(app, "dist", "assets")) ? readdirSync(join(app, "dist", "assets")).filter((file) => file.endsWith(".js")) : [];
        if (!assets.some((file) => readFileSync(join(app, "dist", "assets", file), "utf8").includes("Qualified"))) problems.push("the app's build has none of App.rs's JSX");
      }
    }
  } finally {
    rmSync(app, { recursive: true, force: true });
  }
  const path = join(logs, "vite-app.log");
  writeFileSync(path, [...log, ...problems].join("\n"));
  return {
    name: "vite-app",
    command: ["vite", "build"],
    compiler: "node_modules/.bin/rust-js",
    passed: problems.length === 0,
    pass: problems.length === 0 ? 1 : 0,
    fail: problems.length === 0 ? 0 : 1,
    skip: 0,
    skipped: [],
    seconds: Math.round((Date.now() - started) / 1000),
    log: path,
  };
}

function suite(name: string, tests: string[], compiler: string, logs: string, env: Record<string, string> = {}): Suite {
  const command = [process.execPath, "test", ...tests];
  const started = Date.now();
  const p = runSync(command, root, suiteTimeout, { ...cleared, ...env, RUST_JS_COMPILER: compiler });
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
  if (process.env.BLESS) throw new Error("BLESS is set: qualification checks what the snapshots expect, and doesn't rewrite it");
  const dist = resolve(distArg);
  const reports = resolve(reportArg);
  mkdirSync(reports, { recursive: true });
  // The tests run are the commit's, as they are: nothing changed in the
  // checkout when it starts, and nothing changed by the suite when it ends,
  // but the distribution and the report, if they're in it.
  const changes = () => {
    const outside = [dist, reports].filter((path) => !relative(root, path).startsWith("..")).map((path) => `:!${relative(root, path)}`);
    return text(["git", "status", "--porcelain", "--", ".", ...outside]);
  };
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
  const before = changes();
  if (before) problems.push(`the checkout has changes of its own: ${before.split("\n").slice(0, 3).join(", ")}`);

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
          "@rust-js/runtime": dependency("runtime.tgz"),
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
        suites.push(suite("packaged-binary", ["test/packages.test.ts"], binary, reports, { RUST_JS_DISTRIBUTION: dist }));
        suites.push(viteApp(dist, reports));
      }
    }
  } finally {
    rmSync(project, { recursive: true, force: true });
  }
  for (const s of suites) if (!s.passed) problems.push(`${s.name}: ${s.fail} failed, ${s.pass} passed; see ${s.log}`);
  const after = changes();
  if (after && after !== before) problems.push(`the suite changed the checkout: ${after.split("\n").slice(0, 3).join(", ")}`);

  const report = {
    qualified: problems.length === 0,
    problems,
    commit,
    distribution: manifest,
    host: { platform: process.platform, arch: process.arch, release: release(), cpu: cpus()[0]?.model ?? "", memory: totalmem() },
    runtimes: { bun: Bun.version, node: text(["node", "--version"]), rustc: text(["rustc", "--version"]) },
    suites,
    // What the suite was run with: the settings it leaves out, and so the
    // generated programs it runs, the first 12 seeds.
    settings: { unset, fuzz: { start: 1, seeds: 12 } },
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
