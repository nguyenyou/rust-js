// Native build preparation. Hosts provide scheduling and consume manifests.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { resourceInputs } from "./resources.js";
import { parseCompilerIdentity } from "./manifest.js";

export const defaultResources = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const defaultCompiler = join(defaultResources, "target/debug/rust-js");

function installedResources(root) {
  const require = createRequire(join(root, "package.json"));
  try { return dirname(require.resolve("rust-js-resources/package.json")); }
  catch (error) {
    if (error.code !== "MODULE_NOT_FOUND") throw error;
    return defaultResources;
  }
}

function run(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, RUST_JS_JS_RUNTIME: process.execPath }, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    let errors = "";
    child.stdout.setEncoding("utf8").on("data", chunk => { output += chunk; });
    child.stderr.setEncoding("utf8").on("data", chunk => { errors += chunk; });
    child.on("error", reject);
    child.on("close", code => code === 0 ? resolve(output) : reject(new Error(errors || `${command} exited with ${code}`)));
  });
}

export function createNativeBuilder({ root, rustJs = defaultCompiler, resources = installedResources(root), cacheDir = join(root, "node_modules/.cache/rust-js"), rustcFlags = [], bindings = ["react"], externs = {} }) {
  const repo = resources;
  const compilerPath = existsSync(rustJs) ? realpathSync(rustJs) : rustJs;
  const compilerInputs = [...new Set([rustJs, compilerPath])];
  const nativePackage = join(dirname(compilerPath), "../package.json");
  const packaged = existsSync(nativePackage) && JSON.parse(readFileSync(nativePackage, "utf8")).name === "rust-js-native";
  if (packaged) {
    compilerInputs.push(nativePackage, join(dirname(compilerPath), "compiler"));
  }
  const compilerCommand = packaged ? process.execPath : rustJs;
  const compilerArgs = packaged ? [compilerPath] : [];
  const metadataInputs = resourceInputs(bindings).map(p => join(repo, p));
  // The React the project has installed, whose API the react crate is built
  // with (ADR 0043): what a later React added doesn't compile. `null` without
  // one, which gets the latest's.
  function installedReact() {
    try {
      const require = createRequire(join(root, "package.json"));
      return JSON.parse(readFileSync(require.resolve("react/package.json"), "utf8")).version;
    } catch {
      return null;
    }
  }

  // Each recipe uses the pinned resources and a content-keyed cache directory.
  async function prepare() {
    if (!existsSync(rustJs)) throw new Error(`no rust-js at ${rustJs}: configure rustJs with an installed compiler or build it with cargo build`);
    const react = bindings.includes("react") ? installedReact() : null;
    if (!bindings.length) return { flags: [], react };
    const packagePath = join(repo, "package.json");
    if (existsSync(packagePath)) {
      const resourcePackage = JSON.parse(await readFile(packagePath, "utf8"));
      if (resourcePackage.name === "rust-js-resources") {
        const identity = parseCompilerIdentity(await run(compilerCommand, [...compilerArgs, "--version-json"], root));
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
      if (!existsSync(stamp) || !existsSync(join(metadata, "libreact.rmeta")) || !existsSync(join(metadata, "libweb.rmeta"))) {
        await mkdir(metadata, { recursive: true });
        await run(join(repo, "react/build.sh"), ["-o", join(metadata, "libreact.rmeta"), ...(react ? ["--react", react] : [])], repo);
        await writeFile(stamp, key);
      }
      flags.push("--extern", `react=${join(metadata, "libreact.rmeta")}`, "-L", metadata);
    }
    if (bindings.includes("serde")) {
      const toolchain = readFileSync(join(repo, "rust-toolchain.toml"), "utf8").match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
      if (!toolchain) throw new Error("Binding resources must declare a pinned Rust toolchain");
      // Cargo's structured output handles hashed filenames and paths with spaces.
      // Run Cargo even on reuse: it checks that every dependency is still fresh.
      const output = await run("cargo", [`+${toolchain}`, "build", "--locked", "--message-format=json",
        "--manifest-path", join(repo, "serde/Cargo.toml"), "--target-dir", join(cacheDir, "serde", key)], repo);
      const artifacts = output.split("\n").filter(Boolean).map(line => JSON.parse(line))
        .filter(message => message.reason === "compiler-artifact");
      for (const name of ["serde", "serde_json"]) {
        const artifact = artifacts.find(message => message.target.name === name && message.target.kind.includes("lib"));
        const file = artifact?.filenames.find(file => file.endsWith(".rmeta"))
          ?? artifact?.filenames.find(file => file.endsWith(".rlib"));
        if (!file || !existsSync(file)) throw new Error(`Cargo produced no ${name} metadata`);
        flags.push("--extern", `${name}=${file}`, "-L", `dependency=${dirname(file)}`);
      }
    }
    return { flags, react };
  }

  return {
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
