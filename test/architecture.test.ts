// Architectural dependencies are executable rules, not only a diagram.
import { expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { root } from "./support";

const read = (path: string) => readFileSync(join(root, path), "utf8");
const files = (directory: string): string[] => readdirSync(join(root, directory), { withFileTypes: true }).flatMap(entry =>
  entry.isDirectory() ? files(`${directory}/${entry.name}`) : [`${directory}/${entry.name}`]);

// Every source module is in a layer, so a module added later is checked
// by the rules of its layer, not left out of a list (found in review).
//
//   driver ──► front end (rustc) ──► owned output ──► printing (oxc)
const layers = {
  driver: ["src/main.rs", "src/cargo.rs"],
  front: ["src/lower.rs", "src/lower/", "src/jsx_syntax.rs", "src/jsx_syntax/"],
  owned: ["src/library.rs", "src/reachability.rs", "src/link.rs", "src/names.rs", "src/program.rs", "src/js.rs",
    "src/prepare.rs", "src/output.rs", "src/publish.rs", "src/manifest.rs", "src/runtime.rs", "src/settings.rs", "src/hooks.rs"],
  printing: ["src/to_oxc.rs", "src/format.rs"],
};
const layerOf = (file: string) =>
  Object.entries(layers).find(([, paths]) => paths.some(p => p.endsWith("/") ? file.startsWith(p) : file === p))?.[0];
const sources = files("src").filter(path => path.endsWith(".rs"));

test("every source module is in a layer", () => {
  expect(sources.filter(file => !layerOf(file))).toEqual([]);
  expect(sources.length).toBeGreaterThan(40);
});

test("owned compiler output and downstream phases do not depend on rustc", () => {
  for (const file of sources.filter(file => layerOf(file) === "owned" || layerOf(file) === "printing")) {
    expect(read(file), file).not.toMatch(/(?:use\s+|\b)rustc_\w+::/);
  }
});

test("linking uses owned output without lowering dependencies", () => {
  for (const file of ["src/reachability.rs", "src/link.rs", "src/names.rs"]) {
    expect(read(file), file).not.toMatch(/(?:crate|super)::lower\b/);
  }
});

test("oxc APIs stay behind the printing and formatting adapters", () => {
  for (const file of sources.filter(file => layerOf(file) !== "printing")) {
    expect(read(file), file).not.toMatch(/\boxc_\w+::/);
  }
});

test("lowering and artifact planning cannot publish files", () => {
  for (const file of ["src/lower.rs", ...files("src/lower"), "src/output.rs"]) {
    expect(read(file), file).not.toMatch(/(?:std::fs|fs)::(?:write|rename|remove_file|create_dir|create_dir_all|File::create)\b/);
  }
});

test("library recognition cannot access function emission state", () => {
  for (const file of ["src/lower/recognition.rs", ...files("src/lower/recognition")]) {
    expect(read(file), file).not.toMatch(/\b(?:FnCx|ExprId|Stmt)\b|\bRefCell\s*<|crate::js|runtime::/);
    // HIR visitors accumulate local query results; classification methods
    // must not mutate their shared recognition context.
    expect(read(file), file).not.toMatch(/\bfn\s+(?!visit_)\w+\s*\([^)]*&mut\s+self/);
  }
  expect(read("src/lower/recognition.rs")).toContain("pub(super) struct Recognition");
});

test("body queries cannot access emission state", () => {
  const source = read("src/lower/body_queries.rs");
  expect(source).not.toMatch(/\b(?:FnCx|Dependencies|Evaluation)\b|\bRefCell\s*<|crate::js|runtime::/);
  expect(source).not.toMatch(/\bfn\s+\w+\s*\([^)]*&mut\s+self/);
});

// What an expression can do that can be seen is a question of the THIR
// alone (ADRs 0098, 0139): asked by emission, it never reaches into it.
test("effects analysis cannot access emission state", () => {
  const source = read("src/lower/effects.rs");
  expect(source).not.toMatch(/\b(?:FnCx|Dependencies|Evaluation)\b|\bRefCell\s*<|crate::js|runtime::|super::drops\b/);
  expect(source).not.toMatch(/\bfn\s+\w+\s*\([^)]*&mut\s+self/);
});

// Recognition is what the rest asks: it asks neither the destructors'
// analysis nor the effects one, which ask it.
test("recognition depends on neither destructors nor effects", () => {
  for (const file of ["src/lower/recognition.rs", ...files("src/lower/recognition")]) {
    expect(read(file), file).not.toMatch(/\b(?:super|crate::lower)::(?:drops|effects)\b/);
  }
});

test("library identity checks and method tables stay in recognition", () => {
  for (const file of files("src/lower").filter(file => !file.includes("/recognition") && !file.endsWith("/library.rs"))) {
    // The scalar linkage adapter owns canonical crate names, not library intrinsics.
    expect(read(file), file).not.toMatch(/\.crate_name\s*\(|fn classify(?:_\w+)?\s*\(/);
  }
  for (const file of ["src/lower/calls.rs", "src/lower/ordering.rs", "src/lower/traits.rs"]) {
    expect(read(file), file).not.toMatch(/match\s+(?:self\.tcx\.item_name\(\w+\)|name)\.as_str\(\)/);
  }
});

test("Vite delegates build preparation and validates build results", () => {
  const plugin = read("vite-plugin/index.js");
  expect(plugin).not.toMatch(/react\/build\.sh|libreact\.rmeta|rust-toolchain\.toml|child_process/);
  expect(plugin).toContain("createNativeBuilder");
  expect(plugin).toContain("parseManifest");
});

test("native and WASI compiler dependency versions agree", () => {
  const dependencies = (path: string) => {
    const section = read(path).split("[dependencies]\n")[1].split(/\n\[/)[0];
    return new Map(section.split("\n").filter(line => /^(?:serde|oxc_)/.test(line)).map(line => {
      const equals = line.indexOf("=");
      return [line.slice(0, equals).trim(), line.slice(equals + 1).trim()];
    }));
  };
  expect(dependencies("wasm/Cargo.toml")).toEqual(dependencies("Cargo.toml"));
});

test("crate analysis cannot emit functions or invoke linking", () => {
  expect(read("src/lower/analysis.rs")).not.toMatch(/\b(?:FnCx|CrateFacts|LoweredModule|LoweredFn)\b|\blink::|\.lower_(?:fn|codec|dictionary)\(/);
});

test("lowering returns symbolic output and leaves linking to the driver", () => {
  expect(read("src/lower/pipeline.rs")).not.toMatch(/crate::link|link::|runtime::resolve/);
  expect(read("src/lower/pipeline.rs")).toContain("Option<Unlinked>");
  expect(read("src/main.rs")).toContain("link::link(unlinked)");
});
