import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fingerprint, publishArtifacts } from "../tooling/publish.js";
import { readReport } from "../wasm/web/frame-report.js";

test("WASI host removes only unchanged owned artifacts and validates before publication", () => {
  const dir = mkdtempSync(join(tmpdir(), "rust-js-publish-"));
  try {
    const path = join(dir, "manifest.json");
    const handwritten = join(dir, "handwritten.js");
    writeFileSync(handwritten, "preserve");
    const publish = (values: Record<string, string>) => {
      const files = new Map(Object.entries(values).map(([name, text]) => [join(dir, name), Buffer.from(text)]));
      const manifest = { version: 1, input: join(dir, "lib.rs"), output: join(dir, "lib.js"), sources: [], modules: [],
        artifacts: [...files].map(([file, bytes]) => ({ file, hash: fingerprint(bytes) })) };
      publishArtifacts(path, manifest, files);
      return manifest;
    };
    publish({ "lib.js": "one", "stale.js": "stale", "edited.js": "original" });
    writeFileSync(join(dir, "edited.js"), "user edit");
    const manifest = publish({ "lib.js": "two" });
    expect(existsSync(join(dir, "stale.js"))).toBe(false);
    expect(readFileSync(join(dir, "edited.js"), "utf8")).toBe("user edit");
    expect(readFileSync(handwritten, "utf8")).toBe("preserve");
    expect(() => publishArtifacts(path, manifest, new Map())).toThrow("Missing");
    expect(readFileSync(join(dir, "lib.js"), "utf8")).toBe("two");
    // A later artifact cannot be staged: earlier staged bytes must not leak.
    expect(() => publish({ "lib.js": "three", "handwritten.js/child": "blocked" })).toThrow();
    expect(readFileSync(join(dir, "lib.js"), "utf8")).toBe("two");
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("preview reports reject malformed or ambiguous payloads", () => {
  for (const data of [null, "hello", { run: 1 }, { run: -1, ran: true }, { run: 1, ran: "true" },
    { run: 1, error: {}, ran: true }, { run: 1, tested: { passed: -1, failed: 0, ignored: 0 } }]) {
    expect(readReport({ data })).toBeUndefined();
  }
  for (const data of [{ run: 1, ran: true }, { run: 2, error: "failed" },
    { run: 3, tested: { passed: 3, failed: 0, ignored: 0 } }]) expect(readReport({ data })).toEqual(data);
});
