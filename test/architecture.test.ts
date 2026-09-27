// Architectural dependencies are executable rules, not only a diagram.
import { expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { root } from "./support";

const read = path => readFileSync(join(root, path), "utf8");
const files = directory => readdirSync(join(root, directory), { withFileTypes: true }).flatMap(entry =>
  entry.isDirectory() ? files(`${directory}/${entry.name}`) : [`${directory}/${entry.name}`]);

test("owned compiler output and downstream phases do not depend on rustc", () => {
  for (const file of ["src/link.rs", "src/names.rs", "src/program.rs", "src/js.rs", "src/prepare.rs", "src/output.rs", "src/publish.rs", "src/manifest.rs", "src/to_oxc.rs", "src/format.rs"]) {
    expect(read(file), file).not.toMatch(/(?:use\s+|\b)rustc_\w+::/);
  }
});

test("linking uses owned output without lowering dependencies", () => {
  for (const file of ["src/link.rs", "src/names.rs"]) {
    expect(read(file), file).not.toMatch(/(?:crate|super)::lower\b/);
  }
});

test("oxc APIs stay behind the printing and formatting adapters", () => {
  for (const file of files("src").filter(path => path.endsWith(".rs") && !["src/to_oxc.rs", "src/format.rs"].includes(path))) {
    expect(read(file), file).not.toMatch(/\boxc_\w+::/);
  }
});

test("lowering and artifact planning cannot publish files", () => {
  for (const file of ["src/lower.rs", ...files("src/lower"), "src/output.rs"]) {
    expect(read(file), file).not.toMatch(/(?:std::fs|fs)::(?:write|rename|remove_file|create_dir|create_dir_all|File::create)\b/);
  }
});

test("library recognition cannot access function emission state", () => {
  const recognition = read("src/lower/recognition.rs");
  expect(recognition).not.toMatch(/\b(?:FnCx|ExprId|Stmt)\b|\bRefCell\s*<|crate::js|runtime::|&mut\s+self/);
  expect(recognition).toContain("pub(super) struct Recognition");
});

test("Vite delegates build preparation and validates build results", () => {
  const plugin = read("vite-plugin/index.js");
  expect(plugin).not.toMatch(/react\/build\.sh|libreact\.rmeta|rust-toolchain\.toml|child_process/);
  expect(plugin).toContain("createNativeBuilder");
  expect(plugin).toContain("parseManifest");
});

test("native and WASI compiler dependency versions agree", () => {
  const dependencies = path => {
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
