// Pack the crates rust-js releases on npm, `@rust-js/builtins`,
// `@rust-js/webapi` and `@rust-js/react`: each is Cargo's own packaging of its crate, which
// `package-crates.ts` makes and verifies with a plain stable rustc, in an
// npm package that names the crate. What a crate shares with an app, the
// crates it depends on, is a peer dependency, so an app has one copy of
// each; Cargo finds them in its `node_modules` by a patch. This never
// publishes.
//
//   bun scripts/package-npm-crates.ts <out-dir>   # <out-dir>/builtins.tgz, webapi.tgz, react.tgz

import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const args = Bun.argv.slice(2);
if (args.length !== 1 || args[0].startsWith("-")) throw new Error("Usage: bun scripts/package-npm-crates.ts <out-dir>");
const out = resolve(args[0]);

/** Each crate released on npm, by its directory, and its package's name. */
const packages: Record<string, string> = { builtins: "@rust-js/builtins", webapi: "@rust-js/webapi", react: "@rust-js/react" };

type Manifest = {
  package: {
    name: string;
    version: string;
    description: string;
    license: string;
    /** The npm packages a crate binds, `{ react = ">=18.0.0" }`: its package's peers too. */
    metadata?: { "rust-js"?: { npm?: Record<string, string> } };
  };
  dependencies?: Record<string, { package?: string; version?: string }>;
};

const run = (cmd: string[], cwd = root) => {
  const p = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} failed:\n${p.stderr.toString()}`);
  return p.stdout.toString();
};

const work = mkdtempSync(join(tmpdir(), "rust-js-npm-crates-"));
try {
  // Cargo's packaging, each crate verified, as crates.io would take it.
  const crates = join(work, "crates");
  run([process.execPath, "scripts/package-crates.ts", crates]);
  // The npm package each crate is, by the crate's name.
  const npmName = new Map(Object.entries(packages).map(([dir, name]) => [readManifest(join(root, dir, "Cargo.toml")).package.name, name]));
  mkdirSync(out, { recursive: true });
  for (const [dir, name] of Object.entries(packages)) {
    const { package: crate } = readManifest(join(root, dir, "Cargo.toml"));
    const file = readdirSync(crates).find((f) => f === `${crate.name}-${crate.version}.crate`);
    if (!file) throw new Error(`Cargo didn't package ${crate.name} ${crate.version}`);
    const unpacked = join(work, dir);
    mkdirSync(unpacked);
    run(["tar", "-xzf", join(crates, file), "-C", unpacked]);
    const staging = join(unpacked, `${crate.name}-${crate.version}`);
    // Cargo's record of how it packaged it, which isn't the crate's.
    for (const extra of ["Cargo.toml.orig", ".cargo_vcs_info.json", "Cargo.lock"]) rmSync(join(staging, extra), { force: true });
    copyFileSync(join(root, "LICENSE"), join(staging, "LICENSE"));
    // Its crates' requirements, as Cargo packaged them, are the peer
    // dependencies' ranges: `~0.0.1` means the same to both.
    const manifest = readManifest(join(staging, "Cargo.toml"));
    const peers = Object.fromEntries([
      ...Object.entries(manifest.dependencies ?? {}).flatMap(([key, dep]) => {
        const peer = npmName.get(dep.package ?? key);
        return peer && dep.version ? [[peer, dep.version]] : [];
      }),
      ...Object.entries(manifest.package.metadata?.["rust-js"]?.npm ?? {}),
    ]);
    const pkg = {
      name,
      version: crate.version,
      description: crate.description,
      license: crate.license,
      repository: { type: "git", url: "git+https://github.com/rust-js-lang/rust-js.git", directory: dir },
      keywords: ["rust-js", "rust"],
      "rust-js": { crate: crate.name },
      ...(Object.keys(peers).length ? { peerDependencies: peers } : {}),
      // What Cargo packaged, a build script and what it reads too.
      files: readdirSync(staging).filter((file) => file !== "package.json").sort(),
    };
    writeFileSync(join(staging, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
    run([process.execPath, "pm", "pack", "--ignore-scripts", "--filename", join(out, `${dir}.tgz`)], staging);
    console.log(join(out, `${dir}.tgz`));
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}

function readManifest(path: string): Manifest {
  return Bun.TOML.parse(readFileSync(path, "utf8")) as Manifest;
}
