// Mutations of src/output.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "should-panic-takes-any-throw",
    breaks: "a `#[should_panic]` test passes when the JS throws a `TypeError`, not only when it panics",
    file: "src/output.rs",
    find: "    if (!(e instanceof Error && e.constructor === Error)) {\n",
    replace: "    if (false) {\n",
    tests: ["test/browser.test.ts", "-t", "fails the way Rust's would"],
  },
  {
    name: "library-metadata-unplanned",
    breaks: "a library's metadata is published where it's asked for, unchecked, over a source of the crate",
    file: "src/output.rs",
    find: "                planned.push(path.clone());\n",
    replace: "                drop(path.clone());\n",
    tests: ["test/crates.test.ts", "-t", "a source of the crate"],
  },
];
