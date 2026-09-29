// The compiler's versioned build result. Shared by native and WASI hosts.
function validCompiler(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && typeof value.version === "string" && value.version.length > 0
    && typeof value.toolchain === "string" && value.toolchain.length > 0
    && value.abi === 1;
}

export function parseCompilerIdentity(text) {
  const value = JSON.parse(text);
  if (!validCompiler(value)) throw new Error("Unsupported rust-js compiler identity or ABI; expected ABI 1");
  return value;
}

export function parseManifest(text) {
  let result;
  try { result = JSON.parse(text); }
  catch (error) { throw new Error(`Invalid rust-js manifest JSON: ${error.message}`); }
  const object = value => value !== null && typeof value === "object" && !Array.isArray(value);
  const strings = value => Array.isArray(value) && value.every(item => typeof item === "string");
  const path = value => typeof value === "string" && (/^\//.test(value) || /^[A-Za-z]:[\\/]/.test(value));
  const paths = value => strings(value) && value.every(path);
  const fingerprint = value => object(value) && path(value.file)
    && typeof value.hash === "string" && /^[0-9a-f]{16}$/.test(value.hash);
  if (!object(result) || result.version !== 1) {
    throw new Error(`Unsupported rust-js manifest version ${result?.version}; expected 1`);
  }
  if (result.compiler !== undefined && !validCompiler(result.compiler)) {
    throw new Error("Unsupported rust-js compiler identity or ABI; expected ABI 1");
  }
  if (result.library !== undefined) {
    const library = result.library;
    // ADR 0100: each item another crate can reach, by rustc's key for it.
    if (!object(library) || library.version !== 2 || typeof library.name !== "string" || !library.name
        || typeof library.crate_hash !== "string" || !library.crate_hash
        || !Array.isArray(library.inputs) || !library.inputs.every(fingerprint)
        || !strings(library.impls) || !strings(library.libraries)
        || !Array.isArray(library.items) || !library.items.every(item => object(item)
          && typeof item.key === "string" && typeof item.rust_path === "string" && strings(item.module)
          && typeof item.export === "string" && (item.member === null || typeof item.member === "string")
          && Array.isArray(item.drops) && item.drops.every(Number.isInteger))) {
      throw new Error("Invalid rust-js library contract; expected library ABI 2");
    }
  }
  if (!path(result.input) || !path(result.output) || !paths(result.sources)
      || !Array.isArray(result.modules) || !result.modules.every(module => object(module)
        && strings(module.module) && path(module.file) && path(module.map)
        && (module.source === null || path(module.source)) && paths(module.imports))
      || !Array.isArray(result.artifacts) || !result.artifacts.every(artifact => object(artifact)
        && path(artifact.file) && typeof artifact.hash === "string" && /^[0-9a-f]{16}$/.test(artifact.hash))) {
    throw new Error("Invalid rust-js manifest: expected absolute paths, modules and fingerprinted artifacts");
  }
  const artifacts = new Set(result.artifacts.map(artifact => artifact.file));
  const modules = new Set(result.modules.map(module => module.file));
  if (artifacts.size !== result.artifacts.length
      || modules.size !== result.modules.length
      || result.modules.some(module => !artifacts.has(module.file) || !artifacts.has(module.map)
        || module.imports.some(file => !modules.has(file)))) {
    throw new Error("Invalid rust-js manifest: inconsistent module artifacts or imports");
  }
  return result;
}

// Remap only paths. Names, fingerprints and future unrelated fields stay intact.
export function mapManifestPaths(manifest, map) {
  return {
    ...manifest,
    input: map(manifest.input), output: map(manifest.output),
    sources: manifest.sources.map(map),
    modules: manifest.modules.map(module => ({
      ...module, file: map(module.file), map: map(module.map),
      source: module.source === null ? null : map(module.source), imports: module.imports.map(map),
    })),
    artifacts: manifest.artifacts.map(artifact => ({ ...artifact, file: map(artifact.file) })),
    ...(manifest.library === undefined ? {} : { library: {
      ...manifest.library, inputs: manifest.library.inputs.map(input => ({ ...input, file: map(input.file) })),
    } }),
  };
}
