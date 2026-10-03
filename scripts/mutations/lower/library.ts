// Mutations of src/lower/library.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "library-crate-hash-unchecked",
    breaks: "a library's manifest is taken beside the metadata of another build of it",
    file: "src/lower/library.rs",
    find: "            if let Some(library) = self.dependencies.libraries.get(name.as_str())\n",
    replace: "            if let Some(library) = self.dependencies.libraries.get(name.as_str()).filter(|_| false)\n",
    tests: ["test/crates.test.ts", "-t", "another build"],
  },
];
