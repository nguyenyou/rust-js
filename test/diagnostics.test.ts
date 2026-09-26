import { beforeAll, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, compiler, fixture } from "./support";

beforeAll(buildCompiler, 600_000);

for (const [name, source, message] of [
  ["type error", 'pub fn f() -> i32 { "wrong" }', "mismatched types"],
  ["borrow error", 'pub fn f() -> i32 { let mut x = 1; let r = &x; x = 2; *r }', "borrowed"],
  ["unsupported type", 'pub fn f(x: u64) -> u64 { x }', "does not support"],
  ["malformed import", '#![rust_js::import("./style.css")]\npub fn f() {}', "write it"],
  ["malformed binding", '#[rust_js::link_name(123)] pub fn f() {}', "a binding needs"],
  ["invalid JSX binding", '#[rust_js::link_name = "<div>"] fn div(a: i32, b: i32) -> i32 { unreachable!() }\npub fn f() -> i32 { div(1, 2) }', "JSX binding"],
]) {
  test(`${name} reports a source location and preserves existing output`, () => {
    const dir = fixture("diagnostic");
    const input = join(dir, "lib.rs"), output = join(dir, "lib.js"), manifest = join(dir, "manifest.json");
    writeFileSync(input, source);
    const args = [compiler, input, "-o", output, "--manifest", manifest];
    const rejected = Bun.spawnSync(args);
    expect(rejected.exitCode).not.toBe(0);
    expect(rejected.stderr.toString()).toContain(message);
    expect(rejected.stderr.toString()).toContain("lib.rs:");
    expect(existsSync(output)).toBe(false);
    expect(existsSync(manifest)).toBe(false);
    writeFileSync(output, "last successful build");
    writeFileSync(output + ".map", "previous map");
    expect(Bun.spawnSync(args).exitCode).not.toBe(0);
    expect(readFileSync(output, "utf8")).toBe("last successful build");
    expect(readFileSync(output + ".map", "utf8")).toBe("previous map");
  });
}
