// Mutations of src/lower/analysis/naming.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "import-named-over-export",
    breaks: "an import takes the name of the crate's own function, which is renamed, so JS calling it by its Rust name finds none",
    file: "src/lower/analysis/naming.rs",
    find: "    let mut reserved: HashSet<String> = uses.globals.iter().chain(taken.values().flatten()).cloned().collect();\n",
    replace: "    let mut reserved: HashSet<String> = uses.globals.iter().chain(taken.values().flatten().filter(|_| false)).cloned().collect();\n",
    tests: ["test/crates.test.ts", "-t", "two crates: same_name"],
  },
];
