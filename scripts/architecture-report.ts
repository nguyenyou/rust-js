// How the front end's modules hold to their boundaries, as numbers to read
// before and after a change to them (the architecture audits). The rules
// themselves are test/architecture.test.ts's; this shows where things stand.
//
//   bun scripts/architecture-report.ts
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = join(import.meta.dir, "..");
const files: string[] = [];
const walk = (directory: string) => {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (path.endsWith(".rs")) files.push(path);
  }
};
walk(join(root, "src/lower"));
files.push(join(root, "src/lower.rs"));
const sources = new Map(files.map(file => [relative(root, file).replace("src/lower/", ""), readFileSync(file, "utf8")]));
const names = [...sources.keys()].sort();
const count = (text: string, pattern: RegExp) => (text.match(pattern) ?? []).length;
const section = (title: string, lines: string[]) => console.log(`\n## ${title}\n${lines.join("\n") || "(none)"}`);

section(
  "Largest modules, in lines",
  names
    .map(name => [name, sources.get(name)!.split("\n").length] as const)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 15)
    .map(([name, lines]) => `${String(lines).padStart(5)}  ${name}`),
);

// Each function's state is grouped by concern, and each group has an owner.
const owned: [string, string][] = [
  ["given", "traits.rs"],
  ["writing", "display.rs"],
  ["stepping", "iterators.rs"],
  ["chains", "iterators.rs"],
  ["drop_state", "drops"],
  ["cloning", "std_impls.rs"],
  ["walks", ""],
];
const stateLines: string[] = [];
for (const [group, owner] of owned) {
  const pattern = new RegExp(`self\\s*\\.\\s*${group}\\s*\\.\\s*([a-z_]+)`, "g");
  for (const [name, text] of sources) {
    if (owner && name.includes(owner)) continue;
    const fields = new Map<string, number>();
    for (const match of text.matchAll(pattern)) fields.set(match[1], (fields.get(match[1]) ?? 0) + 1);
    if (fields.size) stateLines.push(`self.${group} in ${name}: ${[...fields].map(([field, n]) => `${field}(${n})`).join(", ")}`);
  }
}
section("State groups read outside their owner (walks: each cache's readers)", stateLines);

const idioms: [string, RegExp][] = [
  ["Iterator.from", /Expr::var\("Iterator"\),\s*"from"/g],
  ["$iter", /"\$iter"|Helper::Iter\b/g],
  ["$someNone", /\$someNone/g],
];
section(
  "JS forms of a concept, by the modules that write them",
  idioms.map(([idiom, pattern]) => {
    const where = names.filter(name => count(sources.get(name)!, pattern) > 0);
    return `${idiom}: ${where.map(name => `${name}(${count(sources.get(name)!, pattern)})`).join(", ")}`;
  }),
);

const identity: [string, RegExp][] = [
  ["is_diagnostic_item", /is_diagnostic_item\(/g],
  ['Symbol::intern("Type")', /Symbol::intern\("[A-Z][A-Za-z]+"\)/g],
  ["item_name(..).as_str() ==", /item_name\([^)]*\)\.as_str\(\)\s*==/g],
  ["def_path_str comparisons", /def_path_str\([^)]*\)(\.as_str\(\))?\s*(==|\.starts_with|\.contains)/g],
];
section(
  "Std identity checked outside recognition.rs and shortcuts.rs",
  identity.map(([check, pattern]) => {
    const where = names
      .filter(name => !/recognition\.rs|shortcuts\.rs/.test(name) && count(sources.get(name)!, pattern) > 0)
      .map(name => [name, count(sources.get(name)!, pattern)] as const)
      .sort((a, b) => b[1] - a[1]);
    const total = where.reduce((sum, [, n]) => sum + n, 0);
    return `${check}: ${total} in ${where.length} modules: ${where.map(([name, n]) => `${name}(${n})`).join(", ")}`;
  }),
);

// Which modules' methods each module calls: how much of the front end it knows.
const owners = new Map<string, string>();
for (const [name, text] of sources) {
  for (const match of text.matchAll(/^\s*pub\(super\) fn ([a-z_0-9]+)/gm)) owners.set(match[1], name);
}
section(
  "Modules each module calls into, most first",
  names
    .map(name => {
      const used = new Set<string>();
      for (const match of sources.get(name)!.matchAll(/self\.([a-z_0-9]+)\(/g)) {
        const owner = owners.get(match[1]);
        if (owner && owner !== name) used.add(owner);
      }
      return [name, used.size] as const;
    })
    .sort((a, b) => b[1] - a[1])
    .slice(0, 12)
    .map(([name, n]) => `${String(n).padStart(3)}  ${name}`),
);
