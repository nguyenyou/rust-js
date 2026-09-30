// Native build preparation. Hosts provide scheduling and consume manifests.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cargoWorkspace, checkCargo } from "./cargo.js";
import { resourceInputs } from "./resources.js";
import { parseCompilerIdentity } from "./manifest.js";

export const defaultResources = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const defaultCompiler = join(defaultResources, "target/debug/rust-js");

export function findCompiler(root) {
  const require = createRequire(join(root, "package.json"));
  try { return join(dirname(require.resolve("@rust-js/native/package.json")), "bin/rust-js"); }
  catch (error) {
    if (error.code !== "MODULE_NOT_FOUND") throw error;
    return defaultCompiler;
  }
}

function installedResources(root) {
  const require = createRequire(join(root, "package.json"));
  try { return dirname(require.resolve("@rust-js/resources/package.json")); }
  catch (error) {
    if (error.code !== "MODULE_NOT_FOUND") throw error;
    return defaultResources;
  }
}

/** The React the project has installed, whose API the react crate is built
 * with (ADR 0043): what a later React added doesn't compile. `null` without
 * one, which gets the latest's. */
function installedReact(root) {
  try {
    const require = createRequire(join(root, "package.json"));
    return JSON.parse(readFileSync(require.resolve("react/package.json"), "utf8")).version;
  } catch {
    return null;
  }
}

/**
 * `@rust-js/runtime` as the project installed it, of `version`, the
 * compiler's: the helpers the JS it writes imports are its release's (ADR
 * 0103). Throws with what to install otherwise.
 */
function checkRuntime(root, version) {
  // Where Node finds a package, from the app up: looked for each time, as
  // it may be installed while a dev server runs.
  let found;
  for (let dir = resolve(root); !found; dir = dirname(dir)) {
    const candidate = join(dir, "node_modules", "@rust-js", "runtime", "package.json");
    if (existsSync(candidate)) found = candidate;
    else if (dirname(dir) === dir) break;
  }
  if (!found) throw new Error(`the JS rust-js ${version} writes imports @rust-js/runtime ${version}: install it`);
  const installed = JSON.parse(readFileSync(found, "utf8")).version;
  if (installed !== version) throw new Error(`rust-js ${version} needs @rust-js/runtime ${version}, not the installed ${installed}`);
}

function run(command, args, cwd, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, RUST_JS_JS_RUNTIME: process.execPath, ...env }, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    let errors = "";
    child.stdout.setEncoding("utf8").on("data", chunk => { output += chunk; });
    child.stderr.setEncoding("utf8").on("data", chunk => { errors += chunk; });
    child.on("error", reject);
    child.on("close", code => code === 0 ? resolve(output) : reject(new Error(errors || `${command} exited with ${code}`)));
  });
}

export function createNativeBuilder({ root, rustJs = findCompiler(root), resources = installedResources(root), cacheDir = join(root, "node_modules/.cache/rust-js"), rustcFlags = [], bindings = ["react"], externs = {} }) {
  const repo = resources;
  const compilerPath = existsSync(rustJs) ? realpathSync(rustJs) : rustJs;
  const compilerInputs = [...new Set([rustJs, compilerPath])];
  const nativePackage = join(dirname(compilerPath), "../package.json");
  const packaged = existsSync(nativePackage) && JSON.parse(readFileSync(nativePackage, "utf8")).name === "@rust-js/native";
  if (packaged) {
    compilerInputs.push(nativePackage, join(dirname(compilerPath), "compiler"));
  }
  const compilerCommand = packaged ? process.execPath : rustJs;
  const compilerArgs = packaged ? [compilerPath] : [];
  const metadataInputs = resourceInputs(bindings).map(p => join(repo, p));
  const pinned = () => readFileSync(join(repo, "rust-toolchain.toml"), "utf8").match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
  // What the compiler says it is, once: `--version-json`.
  let identity;
  const compilerIdentity = async () => (identity ??= parseCompilerIdentity(await run(compilerCommand, [...compilerArgs, "--version-json"], root)));

  // Each recipe uses the pinned resources and a content-keyed cache directory.
  async function prepare() {
    if (!existsSync(rustJs)) throw new Error(`no rust-js at ${rustJs}: configure rustJs with an installed compiler or build it with cargo build`);
    checkRuntime(root, (await compilerIdentity()).version);
    const react = bindings.includes("react") ? installedReact(root) : null;
    if (!bindings.length) return { flags: [], react };
    const packagePath = join(repo, "package.json");
    if (existsSync(packagePath)) {
      const resourcePackage = JSON.parse(await readFile(packagePath, "utf8"));
      if (resourcePackage.name === "@rust-js/resources") {
        const identity = await compilerIdentity();
        const pin = (await readFile(join(repo, "rust-toolchain.toml"), "utf8")).match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
        if (resourcePackage.version !== identity.version || pin !== identity.toolchain) {
          throw new Error(`Incompatible rust-js resources: compiler ${identity.version} (${identity.toolchain}), resources ${resourcePackage.version} (${pin ?? "missing Rust pin"}). Install matching compiler and resources.`);
        }
      }
    }
    const hash = createHash("sha256").update(JSON.stringify({ resources: resolve(resources), react, bindings, rustcFlags }));
    for (const path of compilerInputs) hash.update(await readFile(path));
    for (const path of metadataInputs) hash.update(await readFile(path));
    const key = hash.digest("hex");
    const flags = [];
    if (bindings.includes("react")) {
      const metadata = join(cacheDir, "react", react ?? "latest", key);
      const stamp = join(metadata, "complete");
      if (!existsSync(stamp) || ["libreact.rmeta", "libwebapi.rmeta", "libjs.rmeta"].some(file => !existsSync(join(metadata, file)))) {
        await mkdir(metadata, { recursive: true });
        // The binding crates are this compiler's to compile (ADR 0112).
        await run(join(repo, "react/build.sh"), ["-o", join(metadata, "libreact.rmeta"), ...(react ? ["--react", react] : [])], repo, { RUST_JS_COMPILER: compilerPath });
        await writeFile(stamp, key);
      }
      // React's crates, each a program's to name (ADR 0102): `use js::spawn`.
      for (const name of ["react", "webapi", "js"]) flags.push("--extern", `${name}=${join(metadata, `lib${name}.rmeta`)}`);
      flags.push("-L", metadata);
    }
    if (bindings.includes("serde")) {
      const toolchain = readFileSync(join(repo, "rust-toolchain.toml"), "utf8").match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
      if (!toolchain) throw new Error("Binding resources must declare a pinned Rust toolchain");
      // Cargo's structured output handles hashed filenames and paths with spaces.
      // Run Cargo even on reuse: it checks that every dependency is still fresh.
      // For rust-js's target (ADR 0090); serde_derive, a procedural macro, runs on the host.
      const output = await run("cargo", [`+${toolchain}`, "build", "--locked", "--message-format=json",
        "--target", "wasm32-unknown-unknown",
        "--manifest-path", join(repo, "serde/Cargo.toml"), "--target-dir", join(cacheDir, "serde", key)], repo);
      const artifacts = output.split("\n").filter(Boolean).map(line => JSON.parse(line))
        .filter(message => message.reason === "compiler-artifact");
      // Where the target's libraries are, and where the host's macro is.
      const directories = new Set(artifacts
        .filter(message => message.target.kind.some(kind => kind === "lib" || kind === "rlib" || kind === "proc-macro"))
        .flatMap(message => message.filenames.map(file => dirname(file))));
      for (const name of ["serde", "serde_json"]) {
        const artifact = artifacts.find(message => message.target.name === name && message.target.kind.includes("lib"));
        const file = artifact?.filenames.find(file => file.endsWith(".rmeta"))
          ?? artifact?.filenames.find(file => file.endsWith(".rlib"));
        if (!file || !existsSync(file)) throw new Error(`Cargo produced no ${name} metadata`);
        flags.push("--extern", `${name}=${file}`);
      }
      // Sorted: Cargo reports what it builds as each finishes, and one build's
      // flags must be the next's.
      for (const directory of [...directories].sort()) flags.push("-L", `dependency=${directory}`);
    }
    return { flags, react };
  }

  return {
    /** A Cargo workspace's check (ADR 0101), with the pinned toolchain, this
     * compiler, and the React the project has installed. */
    async checkCargo(options) {
      checkRuntime(root, (await compilerIdentity()).version);
      return checkCargo({ ...options, toolchain: pinned(), compiler: rustJs, react: installedReact(root) ?? undefined });
    },
    /** The workspace of a Cargo manifest, and its target directory. */
    cargoWorkspace(options) {
      return cargoWorkspace({ ...options, toolchain: pinned() });
    },
    watchFiles: [...metadataInputs, ...(bindings.length ? [join(repo, "package.json")] : []), ...compilerInputs, ...Object.values(externs)],
    prepare,
    async compile({ crate, output, manifest }) {
      const { flags, react } = await prepare();
      try {
        await run(compilerCommand, [...compilerArgs, crate, "-o", output, "--manifest", manifest,
          "--", ...flags,
          ...Object.entries(externs).flatMap(([name, file]) => ["--extern", `${name}=${file}`, "-L", dirname(file)]), ...rustcFlags], root);
      } catch (error) {
        if (react && error.message.includes("configured out")) {
          error.message += `\nnote: this project has React ${react}; an item gated \`react = "X.Y"\` needs React X.Y or later\n`;
        }
        throw error;
      }
    },
  };
}
