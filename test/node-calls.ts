// Calls functions of generated JS under Node, the runtime rust-js's JS is
// for (ADR 0095), and writes what each did, as native Rust writes its
// results, for a test to compare. What the functions print isn't kept.
//
//   node test/node-calls.ts calls.json outcomes.json
//
//   calls.json:    { modules: { name: file }, calls: [{ module, fn, args }] }
//   outcomes.json: [{ value } | { panic } | { error }], a call's at its index

import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { decode, encode, observe } from "./oracle.ts";

type Calls = { modules: Record<string, string>; calls: { module: string; fn: string; args: unknown[] }[] };

// Bun runs `.ts` files too, and would pass for Node here.
if (process.versions.bun) throw new Error("node-calls.ts runs under Node, not Bun");
const [callsFile, outcomesFile] = process.argv.slice(2);
const { modules, calls }: Calls = decode(readFileSync(callsFile, "utf8"));
const loaded: Record<string, Record<string, (...args: unknown[]) => unknown>> = {};
for (const [name, file] of Object.entries(modules)) loaded[name] = await import(pathToFileURL(file).href);
writeFileSync(outcomesFile, encode(calls.map((c) => observe(() => loaded[c.module][c.fn](...c.args)))));
