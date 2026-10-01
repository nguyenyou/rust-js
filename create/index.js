#!/usr/bin/env node
// `bun create @rust-js my-app` (ADR 0105): the vite-react example, packed
// beside this as `template/`, copied into a new directory and named for it,
// with rust-js's packages at this release's version, or with `--local`, a
// distribution's, before a release is on npm.
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const self = JSON.parse(readFileSync(join(here, "package.json"), "utf8"));

function fail(message) {
  console.error(message);
  process.exit(1);
}

const args = process.argv.slice(2);
const at = args.indexOf("--local");
const local = at >= 0 ? args.splice(at, 2)[1] : undefined;
if (at >= 0 && !local) fail("--local needs a distribution, the directory `bun run pack:distribution` makes");
const [directory, ...extra] = args;
if (!directory || extra.length) fail("usage: bun create @rust-js@latest <directory> [--local <distribution>]");

// A distribution is for one host, and one compiler: this one's.
const dist = local && resolve(local);
if (dist) {
  const manifest = join(dist, "distribution.json");
  if (!existsSync(manifest)) fail(`${dist} has no distribution.json: give --local the directory \`bun run pack:distribution\` makes`);
  const { platform, arch, compiler } = JSON.parse(readFileSync(manifest, "utf8"));
  if (platform !== process.platform || arch !== process.arch) {
    fail(`${dist} is for ${platform}-${arch}, and this is ${process.platform}-${process.arch}`);
  }
  if (compiler?.version !== self.version) fail(`${dist} is rust-js ${compiler?.version}, and this is ${self.version}'s`);
}

// Never over what's there.
const app = resolve(directory);
if (existsSync(app) && readdirSync(app).length > 0) fail(`${directory} isn't empty: choose a new directory`);
mkdirSync(app, { recursive: true });
cpSync(join(here, "template"), app, { recursive: true });
// npm leaves a package's `.gitignore` out, so it's packed as `_gitignore`.
renameSync(join(app, "_gitignore"), join(app, ".gitignore"));

// Named for its directory, and rust-js's packages, the workspace's in the
// example, this release's: a distribution's archive is its name in the scope.
const path = join(app, "package.json");
const manifest = JSON.parse(readFileSync(path, "utf8"));
manifest.name = basename(app);
const archive = (name) => `file:${join(dist, `${name.replace("@rust-js/", "")}.tgz`)}`;
for (const field of ["dependencies", "devDependencies"]) {
  for (const [name, spec] of Object.entries(manifest[field] ?? {})) {
    if (spec.startsWith("workspace:")) manifest[field][name] = dist ? archive(name) : self.version;
  }
}
// The compiler is the app's own, as the plugin finds it, from the app: a
// release's or a distribution's. A distribution's `@rust-js/build` is the
// app's too, and the plugin's, where a release's is the registry's.
manifest.devDependencies["@rust-js/native"] = dist ? archive("@rust-js/native") : self.version;
if (dist) {
  manifest.devDependencies["@rust-js/build"] = archive("@rust-js/build");
  manifest.overrides = { ...manifest.overrides, "@rust-js/build": archive("@rust-js/build") };
}
for (const field of ["dependencies", "devDependencies"]) {
  manifest[field] = Object.fromEntries(Object.entries(manifest[field]).sort(([a], [b]) => a.localeCompare(b)));
}
writeFileSync(path, JSON.stringify(manifest, null, 2) + "\n");

// The app is Bun's or Node's: its commands are the package manager's that
// ran this, as npm, Bun, pnpm and Yarn each say in `npm_config_user_agent`,
// `bun/1.4.2 npm/? node/v24.3.0 ..`, or else the runtime's.
const agent = process.env.npm_config_user_agent?.split("/")[0];
const manager = agent || (process.versions.bun ? "bun" : "npm");
const { toolchain } = self.rustJs;
console.log(`Made ${directory}: a Vite and React app, its component src/App.rs, in Rust.

rust-js runs with rustc's own libraries, of the Rust release it's built with. Once:

  rustup toolchain install ${toolchain} --profile minimal --target wasm32-unknown-unknown

Then:

  cd ${directory}
  ${manager} install
  ${manager} run dev
`);
