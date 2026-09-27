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
  if (!object(result) || result.version !== 1) {
    throw new Error(`Unsupported rust-js manifest version ${result?.version}; expected 1`);
  }
  if (result.compiler !== undefined && !validCompiler(result.compiler)) {
    throw new Error("Unsupported rust-js compiler identity or ABI; expected ABI 1");
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
  };
}
