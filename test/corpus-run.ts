// Runs a corpus case's JS as its native binary runs: what `main` prints is
// this process's output, and how it ended, `{ value }` or `{ panic }`, goes
// to the file named second. Node runs it (it strips types); so can Bun.
//
//   bun test/corpus-run.ts case.js outcome.json
//   node test/corpus-run.ts case.js outcome.json

import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { observe } from "./oracle.ts";

const [module, outcomeFile] = process.argv.slice(2);
const { entry } = await import(pathToFileURL(module).href);
writeFileSync(outcomeFile, JSON.stringify(observe(entry)));
