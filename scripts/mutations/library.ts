// Mutations of src/library.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "library-libraries-unchecked",
    breaks: "a consumer given a library but not the libraries that one was compiled against is compiled anyway",
    file: "src/library.rs",
    find: "        if let Some((library, used)) = needed.iter().find(|(_, used)| !result.libraries.contains_key(used)) {\n",
    replace: "        if let Some((library, used)) = needed.iter().find(|_| false) {\n",
    tests: ["test/crates.test.ts", "-t", "not the libraries"],
  },
];
