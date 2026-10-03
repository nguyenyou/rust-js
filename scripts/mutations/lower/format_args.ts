// Mutations of src/lower/format_args.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "begin-panic-payload",
    breaks: "`panic!(5)` before edition 2021 throws a message Rust never shows",
    file: "src/lower/format_args.rs",
    find: "                if !text {",
    replace: "                if false && !text {",
    tests: ["test/corpus.test.ts", "-t", "begin_panic_value"],
  },
  {
    name: "format-dyn-debug-plain",
    breaks: "a `&dyn Debug` made in a format's arguments for its `{:#?}`, as `dbg!` makes one, is shown plain",
    file: "src/lower/format_args.rs",
    find: "        } else {\n            Pretty::Always\n        };",
    replace: "        } else {\n            Pretty::Plain\n        };",
    tests: ["test/corpus.test.ts", "-t", "pretty_debug"],
  },
  {
    name: "kept-dyn-debug-pretty",
    breaks: "`{:#?}` of a `&dyn Debug` made elsewhere, already a plain string, is shown plain",
    file: "src/lower/format_args.rs",
    find: "            if !made_here || !plain_dyn.is_empty() {",
    replace: "            if false && (!made_here || !plain_dyn.is_empty()) {",
    tests: ["test/traits.test.ts", "-t", "kept &dyn Debug"],
  },
];
