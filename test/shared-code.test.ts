import { expect, test } from "bun:test";
import { mkdtempSync, realpathSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createNativeBuilder } from "../tooling/build.js";
import { parseManifest } from "../tooling/manifest.js";
import { buildCompiler, compiler, root as repository, run } from "./support";

test("independent native and JS clients share models, validation, and source edits", async () => {
  buildCompiler();
  const root = realpathSync(mkdtempSync(join(tmpdir(), "rust-js shared app ")));
  try {
    for (const dir of ["shared", "client", "server"]) mkdirSync(join(root, dir));
    const model = join(root, "shared/model.rs");
    const client = join(root, "client/lib.rs");
    const server = join(root, "server/main.rs");
    const binary = join(root, "server-native");
    const output = join(root, "generated/client.js");
    const manifest = join(root, "manifest.json");
    const definitions = (limit: number) => `
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Request { pub count: u32 }
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Reply { pub accepted: bool, pub count: u32 }
pub fn valid(count: u32) -> bool { count <= ${limit} }
pub fn handle(text: &str) -> String {
    let reply = match serde_json::from_str::<Request>(text) {
        Ok(request) => Reply { accepted: valid(request.count), count: request.count },
        Err(_) => Reply { accepted: false, count: 0 },
    };
    serde_json::to_string(&reply).unwrap()
}
`;
    writeFileSync(client, `
#[path = "../shared/model.rs"]
mod model;
pub fn request(count: u32) -> String {
    serde_json::to_string(&model::Request { count }).unwrap()
}
pub fn valid(count: u32) -> bool { model::valid(count) }
pub fn handle(text: &str) -> String { model::handle(text) }
pub fn accepted(text: &str) -> bool {
    let reply: model::Reply = serde_json::from_str(text).unwrap();
    reply.accepted
}
`);
    writeFileSync(server, `
#[path = "../shared/model.rs"]
mod model;
fn main() {
    println!("{}", model::handle(&std::env::args().nth(1).unwrap()));
}
`);
    const builder = createNativeBuilder({ root, rustJs: compiler, bindings: ["serde"] });
    const { flags } = await builder.prepare();
    // Cargo build emits both metadata and linkable libraries. Native code
    // needs the latter; rust-js only needs metadata to run rustc's checks.
    const nativeFlags = flags.map(flag => flag.replace(/\.rmeta$/, ".rlib"));
    for (const flag of nativeFlags.filter(flag => /^(serde|serde_json)=/.test(flag))) {
      expect(existsSync(flag.slice(flag.indexOf("=") + 1))).toBe(true);
    }
    const pin = Bun.TOML.parse(readFileSync(join(repository, "rust-toolchain.toml"), "utf8")).toolchain.channel;
    // Fresh processes also reload imported model modules after the shared edit.
    const callClient = (method: string, value: string | number) => JSON.parse(run([
      process.execPath, "-e",
      `const client = await import(${JSON.stringify(output)}); console.log(JSON.stringify(client[${JSON.stringify(method)}](${JSON.stringify(value)})));`,
    ]));
    for (const limit of [10, 3]) {
      writeFileSync(model, definitions(limit));
      await builder.compile({ crate: client, output, manifest });
      run(["rustc", `+${pin}`, "--edition=2024", "--crate-name", "server", server, "-o", binary, ...nativeFlags]);
      expect(parseManifest(readFileSync(manifest, "utf8")).sources).toContain(model);
      for (const count of [0, 3, 5, 10, 11]) {
        const request = callClient("request", count);
        expect(JSON.parse(request)).toEqual({ count });
        const reply = run([binary, request]).trim();
        expect(JSON.parse(reply)).toEqual({ accepted: count <= limit, count });
        expect(callClient("handle", request)).toBe(reply);
        expect(callClient("accepted", reply)).toBe(count <= limit);
        expect(callClient("valid", count)).toBe(count <= limit);
      }
      for (const invalid of ["{", "{}", '{"count":"bad"}', '{"count":-1}']) {
        const reply = run([binary, invalid]).trim();
        expect(JSON.parse(reply)).toEqual({ accepted: false, count: 0 });
        expect(callClient("handle", invalid)).toBe(reply);
      }
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);
