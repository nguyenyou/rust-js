// Native build preparation. Hosts provide scheduling and consume manifests.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const defaultResources = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const defaultCompiler = join(defaultResources, "target/debug/rust-js");

function run(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: ["ignore", "ignore", "pipe"] });
    let errors = "";
    child.stderr.setEncoding("utf8").on("data", chunk => { errors += chunk; });
    child.on("error", reject);
    child.on("close", code => code === 0 ? resolve() : reject(new Error(errors || `${command} exited with ${code}`)));
  });
}

export function createNativeBuilder({ root, rustJs = defaultCompiler, resources = defaultResources, cacheDir = join(root, "node_modules/.cache/rust-js"), rustcFlags = [], bindings = ["react"], externs = {} }) {
  const unsupported = bindings.filter(name => name !== "react");
  if (unsupported.length) throw new Error(`Unsupported built-in bindings: ${unsupported.join(", ")}; supply explicit externs instead`);
  const repo = resources;
  const metadataInputs = (bindings.includes("react") ? [
    "rust-toolchain.toml", "react/build.sh", "react/cfg.ts", "react/versions.json",
    "react/src/lib.rs", "react/src/event.rs", "react/src/dom.rs", "react/src/elements.rs",
    "web/build.sh", "web/src/lib.rs",
  ] : []).map(p => join(repo, p));
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

  // web's and react's metadata for that React, each version in its own folder.
  let metadata, react;
  async function prepare() {
    if (!existsSync(rustJs)) throw new Error(`no rust-js at ${rustJs}: run bun run build in the rust-js repository`);
    react = installedReact();
    if (!bindings.includes("react")) return;
    const hash = createHash("sha256").update(JSON.stringify({ resources: resolve(resources), react, bindings, rustcFlags }));
    hash.update(await readFile(rustJs));
    for (const path of metadataInputs) hash.update(await readFile(path));
    const key = hash.digest("hex");
    metadata = join(cacheDir, "react", react ?? "latest", key);
    const stamp = join(metadata, "complete");
    if (existsSync(stamp) && existsSync(join(metadata, "libreact.rmeta")) && existsSync(join(metadata, "libweb.rmeta"))) return;
    await mkdir(metadata, { recursive: true });
    await run(join(repo, "react/build.sh"), ["-o", join(metadata, "libreact.rmeta"), ...(react ? ["--react", react] : [])], repo);
    await writeFile(stamp, key);
  }

  return {
    watchFiles: [...metadataInputs, rustJs, ...Object.values(externs)],
    prepare,
    async compile({ crate, output, manifest }) {
      await prepare();
      try {
        await run(rustJs, [crate, "-o", output, "--manifest", manifest,
          "--", ...(bindings.includes("react") ? ["--extern", `react=${join(metadata, "libreact.rmeta")}`, "-L", metadata] : []),
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
