// Shared by cache invalidation and resource packaging: every prepared input
// must travel with the bundle and participate in the metadata cache key.
export const bindingInputs = {
  react: [
    "react/build.sh", "react/cfg.js", "react/versions.json",
    "react/src/lib.rs", "react/src/event.rs", "react/src/dom.rs", "react/src/elements.rs",
    "react/Cargo.toml", "react/build.rs",
    "webapi/build.sh", "webapi/src/lib.rs", "webapi/Cargo.toml",
    "js/build.sh", "js/src/lib.rs", "js/Cargo.toml",
  ],
  serde: ["serde/Cargo.toml", "serde/Cargo.lock", "serde/src/lib.rs"],
};

export function resourceInputs(bindings) {
  return [...new Set([
    ...(bindings.length ? ["rust-toolchain.toml"] : []),
    ...bindings.flatMap(name => {
      if (!Object.hasOwn(bindingInputs, name)) throw new Error(`Unsupported built-in binding: ${name}; supply explicit externs instead`);
      return bindingInputs[name];
    }),
  ])];
}
