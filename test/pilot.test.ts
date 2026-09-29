// The pilot (ROADMAP M3.3, `examples/pilot`): a contacts app whose models and
// their rules are one crate, used by a native server and by a React client
// rust-js compiles, as Cargo checks the workspace (ADR 0101). Real Vite, a
// real browser, the server running: every flow the roadmap names.

import { afterAll, beforeAll, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { chromium, type Browser, type Locator, type Page } from "@playwright/test";
import { createServer, type ViteDevServer } from "vite";
import { buildCompiler, buildSerde, root, run } from "./support";

const pilot = join(root, "examples", "pilot");
const pin = readFileSync(join(root, "rust-toolchain.toml"), "utf8").match(/channel = "([^"]+)"/)![1];

let api: ReturnType<typeof Bun.spawn> | undefined;
let address = "";
let vite: ViteDevServer | undefined;
let browser: Browser | undefined;

/** The native server, on a port of its own: where it listens. */
async function startApi(): Promise<string> {
  api = Bun.spawn([join(pilot, "target", "debug", "server")], { env: { ...process.env, PORT: "0" }, stdout: "pipe", stderr: "inherit" });
  const reader = (api.stdout as ReadableStream<Uint8Array>).getReader();
  let text = "";
  while (!text.includes("\n")) {
    const { value, done } = await reader.read();
    if (done) throw new Error("the server ended before it listened");
    text += new TextDecoder().decode(value);
  }
  reader.releaseLock();
  return text.match(/listening on (\S+)/)![1];
}

beforeAll(async () => {
  buildCompiler();
  // serde and serde_json, fetched as serde/build.sh locks them, which the
  // pilot's lockfile has too: its build is offline.
  buildSerde();
  run(["cargo", `+${pin}`, "build", "--offline", "--quiet", "-p", "server", "--manifest-path", join(pilot, "Cargo.toml")]);
  address = await startApi();
  process.env.PILOT_API = address;
  vite = await createServer({ root: join(pilot, "web"), configFile: join(pilot, "web", "vite.config.js"), logLevel: "silent", server: { port: 0 } });
  await vite.listen();
  browser = await chromium.launch({ headless: true });
}, 600_000);

afterAll(async () => {
  await browser?.close();
  await vite?.close();
  api?.kill();
  delete process.env.PILOT_API;
});

/** What `names` shows, once it's `expected`, or what it last was. */
async function settled(names: Locator, expected: string[]): Promise<string[]> {
  let last: string[] = [];
  for (let i = 0; i < 100; i++) {
    last = await names.allTextContents();
    if (JSON.stringify(last) === JSON.stringify(expected)) break;
    await Bun.sleep(50);
  }
  return last;
}

async function open(hash = "#/"): Promise<Page> {
  const page = await browser!.newPage();
  await page.goto(vite!.resolvedUrls!.local[0] + hash);
  return page;
}

test("the list loads from the server, and a search shows only what matches", async () => {
  const page = await open();
  const names = page.locator(".contacts li a");
  await names.first().waitFor();
  expect(await names.allTextContents()).toEqual(["Ada Lovelace", "Alan Turing", "Grace Hopper"]);
  await page.getByLabel("Search").fill("gr");
  await page.locator(".contacts li a", { hasText: "Grace Hopper" }).waitFor();
  expect(await settled(names, ["Grace Hopper"])).toEqual(["Grace Hopper"]);
  await page.getByLabel("Search").fill("nobody");
  await page.getByText("No contacts match.").waitFor();
  await page.close();
}, 60_000);

// A search's answer that comes after a newer one's must never show: the
// older request is aborted.
test("a slow answer to an older search doesn't replace a newer one's", async () => {
  const page = await open();
  await page.locator(".contacts li a").first().waitFor();
  let slow = 0;
  await page.route(/\/api\/contacts\?q=a$/, async (route) => {
    slow++;
    await Bun.sleep(1_000);
    await route.continue().catch(() => {});
  });
  await page.getByLabel("Search").fill("a");
  await page.getByLabel("Search").fill("alan");
  const names = page.locator(".contacts li a");
  await page.locator(".contacts li a", { hasText: "Alan Turing" }).waitFor();
  expect(await settled(names, ["Alan Turing"])).toEqual(["Alan Turing"]);
  await Bun.sleep(1_500);
  expect(slow).toBe(1);
  expect(await names.allTextContents()).toEqual(["Alan Turing"]);
  await page.close();
}, 60_000);

test("a contact is its own page, and one that doesn't exist says so", async () => {
  const page = await open();
  await page.getByRole("link", { name: "Grace Hopper" }).click();
  await page.getByRole("heading", { name: "Grace Hopper" }).waitFor();
  expect(page.url()).toEndWith("#/contacts/3");
  expect(await page.locator("dd").allTextContents()).toEqual(["grace@example.com", "85"]);
  await page.goto(vite!.resolvedUrls!.local[0] + "#/contacts/99");
  await page.getByText("There's no contact 99.").waitFor();
  await page.goto(vite!.resolvedUrls!.local[0] + "#/nowhere");
  await page.getByText("There's no such page.").waitFor();
  await page.close();
}, 60_000);

// The form checks what the server would, before it sends; what only the
// server knows, a taken email, comes back by field; and a new contact is
// its page, announced by the npm package's toast.
test("the form checks as the server does, shows the server's errors, and adds a contact", async () => {
  const page = await open("#/new");
  const posts: string[] = [];
  page.on("request", (request) => { if (request.method() === "POST") posts.push(request.postData() ?? ""); });
  const submit = page.getByRole("button", { name: "Add contact" });
  await page.getByLabel("Email").fill("not an email");
  await page.getByLabel("Age").fill("old");
  await submit.click();
  const errors = page.locator(".field-error");
  await errors.first().waitFor();
  expect(await errors.allTextContents()).toEqual(['a name is required', '"not an email" isn\'t an email address', '"old" isn\'t a whole number']);
  expect(await page.getByLabel("Email").getAttribute("aria-invalid")).toBe("true");
  expect(posts).toEqual([]);

  await page.getByLabel("Name").fill("Ada Again");
  await page.getByLabel("Email").fill("ADA@example.com");
  await page.getByLabel("Age").fill("30");
  await submit.click();
  await page.getByText("ADA@example.com is taken").waitFor();
  expect(posts.map((body) => JSON.parse(body))).toEqual([{ name: "Ada Again", email: "ADA@example.com", age: 30 }]);

  await page.getByLabel("Name").fill("  Katherine Johnson ");
  await page.getByLabel("Email").fill("katherine@example.com");
  await page.getByLabel("Age").fill("101");
  await submit.click();
  await page.getByRole("heading", { name: "Katherine Johnson" }).waitFor();
  await page.getByText("Added Katherine Johnson").waitFor();
  expect(page.url()).toMatch(/#\/contacts\/\d+$/);
  await page.getByRole("link", { name: "Contacts", exact: true }).click();
  await page.getByRole("link", { name: "Katherine Johnson" }).waitFor();
  await page.close();
}, 60_000);

// The server holds a request that isn't the form's to the same rules, and
// JSON that isn't a contact is refused, each with the server's reason.
test("the server refuses invalid JSON and a contact breaking the rules", async () => {
  const post = (body: string) => fetch(`${address}/api/contacts`, { method: "POST", headers: { "content-type": "application/json" }, body });
  const broken = await post("{ not json");
  expect(broken.status).toBe(400);
  expect((await broken.json()).message).toStartWith("invalid JSON");
  const wrongType = await post('{"name":"Bo","email":"bo@example.com","age":"ten"}');
  expect(wrongType.status).toBe(400);
  const invalid = await post('{"name":" ","email":"bo","age":5}');
  expect(invalid.status).toBe(422);
  expect((await invalid.json()).errors.map((e: { field: string }) => e.field)).toEqual(["name", "email", "age"]);
}, 60_000);

// Last: it stops the server. The list says why it has nothing, and loads
// once the server is back.
test("the list says when the server can't answer, and tries again", async () => {
  api!.kill();
  await api!.exited;
  const page = await open();
  await page.getByRole("alert").waitFor();
  expect(await page.getByRole("alert").textContent()).toContain("the server answered");
  const port = new URL(address).port;
  api = Bun.spawn([join(pilot, "target", "debug", "server")], { env: { ...process.env, PORT: port }, stdout: "ignore", stderr: "inherit" });
  await Bun.sleep(500);
  await page.getByRole("button", { name: "Try again" }).click();
  await page.getByRole("link", { name: "Ada Lovelace" }).waitFor();
  await page.close();
}, 60_000);
