// `bun create @rust-js my-app` (ADR 0105): @rust-js/create makes a Vite and
// React app written in Rust, the vite-react example's files, named for its
// directory, with rust-js's packages at the release's version.

import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { runSync } from "./child";
import { fixture, root, run } from "./support";

const pkg = JSON.parse(readFileSync(join(root, "create", "package.json"), "utf8"));
const version = readFileSync(join(root, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)![1];
const pin = readFileSync(join(root, "rust-toolchain.toml"), "utf8").match(/^channel\s*=\s*"([^"]+)"/m)![1];

/** @rust-js/create as a registry would give it: packed, then unpacked. */
function packed(): string {
  const dir = fixture("create-packed");
  run([process.execPath, "scripts/package-create.ts", join(dir, "create.tgz")]);
  run(["tar", "-xzf", join(dir, "create.tgz"), "-C", dir]);
  return join(dir, "package", "index.js");
}

/** Every file under `dir`, relative to it, sorted. */
function files(dir: string): string[] {
  const out: string[] = [];
  const walk = (d: string) => {
    for (const name of readdirSync(d)) {
      const path = join(d, name);
      if (statSync(path).isDirectory()) walk(path);
      else out.push(relative(dir, path));
    }
  };
  walk(dir);
  return out.sort();
}

// Released with the compiler, so the app it makes names that release's
// packages, and the Rust it says to install is the compiler's.
test("@rust-js/create is the compiler's version, and says its Rust pin", () => {
  expect([pkg.name, pkg.version, pkg.rustJs.toolchain]).toEqual(["@rust-js/create", version, pin]);
});

test("a packed @rust-js/create makes the vite-react example's app, named for its directory", () => {
  const index = packed();
  const cwd = fixture("create-app");
  const made = runSync([process.execPath, index, "my-app"], cwd, 60_000);
  expect(made.code).toBe(0);
  const app = join(cwd, "my-app");
  // The example's files, as git has them: not what building it made.
  const example = run(["git", "ls-files", "examples/vite-react"]).trim().split("\n")
    .map((file) => relative("examples/vite-react", file)).sort();
  expect(files(app)).toEqual(example);
  // npm leaves a package's `.gitignore` out; the app has its own.
  expect(readFileSync(join(app, ".gitignore"), "utf8")).toBe(readFileSync(join(root, "examples/vite-react/.gitignore"), "utf8"));
  // The app's README is the app's, not this repository's example's.
  expect(readFileSync(join(app, "README.md"), "utf8")).not.toContain("repository root");
  const manifest = JSON.parse(readFileSync(join(app, "package.json"), "utf8"));
  expect(manifest.name).toBe("my-app");
  expect(manifest.dependencies["@rust-js/runtime"]).toBe(version);
  expect(manifest.devDependencies["@rust-js/vite-plugin"]).toBe(version);
  // The compiler, which a release of @rust-js/build doesn't bring: the app's own.
  expect(manifest.devDependencies["@rust-js/native"]).toBe(version);
  expect(JSON.stringify(manifest)).not.toContain("workspace:");
  // What to run next, the Rust it needs first.
  expect(made.stdout).toContain(`rustup toolchain install ${pin} --profile minimal --target wasm32-unknown-unknown
`);
  expect(made.stdout).toContain("cd my-app");
  expect(made.stdout).toContain("bun install");
});

// An editor's rust-analyzer, or a `cargo check`, checks the app's Rust as
// Cargo does: its Cargo.toml has the crates it uses where the app installs
// them, in @rust-js/resources, as the packed package has them.
test("a created app's Cargo.toml checks with the crates @rust-js/resources has", () => {
  const index = packed();
  const cwd = fixture("create-cargo");
  expect(runSync([process.execPath, index, "app"], cwd, 60_000).code).toBe(0);
  const app = join(cwd, "app");
  expect(JSON.parse(readFileSync(join(app, "package.json"), "utf8")).devDependencies["@rust-js/resources"]).toBe(version);
  // As a package manager installs it: the packed package, unpacked.
  const scope = join(app, "node_modules", "@rust-js");
  mkdirSync(scope, { recursive: true });
  run([process.execPath, "scripts/package-resources.ts", join(cwd, "resources.tgz")]);
  run(["tar", "-xzf", join(cwd, "resources.tgz"), "-C", scope]);
  renameSync(join(scope, "package"), join(scope, "resources"));
  const check = runSync(["cargo", "check", "--offline", "--quiet", "--manifest-path", join(app, "Cargo.toml")], app, 300_000, { RUSTC_BOOTSTRAP: undefined });
  // No error, and no warning of the crates it uses: the app's are of what's
  // used only in JSX, which react's placeholder `jsx!` doesn't look inside.
  const warnings = check.stderr.split("\n").filter((line) => /^(warning|error)\b/.test(line) && !/\(lib\) generated \d+ warnings?/.test(line));
  expect(warnings.filter((line) => !/^warning: (unused variable: |static `\w+` is never used)/.test(line))).toEqual([]);
  expect(check.stderr).not.toMatch(/`rust-js-\w+` \(lib\) generated/);
  expect(check.code).toBe(0);
}, 300_000);

// Before a release is on npm: the packages `pack:distribution` made, each
// the app's, and `@rust-js/build` the plugin's too, as Qualify installs them.
// An app is Bun's or Node's: what to run next is the package manager's
// that ran it, as it says in `npm_config_user_agent`, or else the runtime's.
test("@rust-js/create says the commands of the package manager that ran it", () => {
  const index = packed();
  const next = (agent: string | undefined, runtime = process.execPath) => {
    const cwd = fixture("create-agent");
    const made = runSync([runtime, index, "app"], cwd, 60_000, { npm_config_user_agent: agent });
    expect(made.code).toBe(0);
    return made.stdout.slice(made.stdout.indexOf("cd app")).trimEnd();
  };
  expect(next("npm/10.9.2 node/v22.12.0 darwin arm64 workspaces/false")).toBe("cd app\n  npm install\n  npm run dev");
  expect(next("bun/1.4.2 npm/? node/v24.3.0 darwin arm64")).toBe("cd app\n  bun install\n  bun run dev");
  expect(next("pnpm/10.12.1 npm/? node/v22.12.0 linux x64")).toBe("cd app\n  pnpm install\n  pnpm run dev");
  // Run by Node itself, `node index.js app`: npm's.
  expect(next(undefined, "node")).toBe("cd app\n  npm install\n  npm run dev");
});

// What its README says to install is what it prints, at this pin.
test("@rust-js/create's README says to install the Rust it prints", () => {
  const readme = readFileSync(join(root, "create", "README.md"), "utf8");
  expect(readme).toContain(`rustup toolchain install ${pin} --profile minimal --target wasm32-unknown-unknown`);
});

test("--local makes the app install a distribution's packages", () => {
  const index = packed();
  const dist = fixture("create-dist");
  writeFileSync(join(dist, "distribution.json"), JSON.stringify({ version: 1, platform: process.platform, arch: process.arch, compiler: { version } }));
  const cwd = fixture("create-local");
  const made = runSync([process.execPath, index, "app", "--local", dist], cwd, 60_000);
  expect(made.code).toBe(0);
  const manifest = JSON.parse(readFileSync(join(cwd, "app", "package.json"), "utf8"));
  const file = (name: string) => `file:${join(dist, name)}`;
  expect(manifest.dependencies["@rust-js/runtime"]).toBe(file("runtime.tgz"));
  expect(manifest.devDependencies).toMatchObject({
    "@rust-js/vite-plugin": file("vite-plugin.tgz"),
    "@rust-js/build": file("build.tgz"),
    "@rust-js/native": file("native.tgz"),
    "@rust-js/resources": file("resources.tgz"),
  });
  expect(manifest.overrides).toEqual({ "@rust-js/build": file("build.tgz") });
});

test("--local refuses a directory that isn't a distribution", () => {
  const index = packed();
  const cwd = fixture("create-no-dist");
  const made = runSync([process.execPath, index, "app", "--local", cwd], cwd, 60_000);
  expect(made.code).not.toBe(0);
  expect(made.stderr).toContain("distribution.json");
  expect(existsSync(join(cwd, "app"))).toBe(false);
});

// It never writes over what's there.
test("@rust-js/create refuses a directory that isn't empty", () => {
  const index = packed();
  const cwd = fixture("create-taken");
  mkdirSync(join(cwd, "taken"));
  writeFileSync(join(cwd, "taken", "notes.txt"), "mine\n");
  const made = runSync([process.execPath, index, "taken"], cwd, 60_000);
  expect(made.code).not.toBe(0);
  expect(made.stderr).toContain("isn't empty");
  expect(readdirSync(join(cwd, "taken"))).toEqual(["notes.txt"]);
});

test("@rust-js/create without a directory says how to use it", () => {
  const index = packed();
  const made = runSync([process.execPath, index], fixture("create-usage"), 60_000);
  expect(made.code).not.toBe(0);
  expect(made.stderr).toContain("bun create @rust-js <directory>");
});
