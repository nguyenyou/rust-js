// Mutations of src/runtime/channel.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "recv-disconnect-ignored",
    breaks: "`recv` of an empty channel whose senders are all dropped panics, where Rust's is an `Err`",
    file: "src/runtime/channel.js",
    find: "  if (channel.senders === 0) return { TAG: \"Err\", _0: undefined };\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "channels"],
  },
  {
    name: "send-receiver-dropped",
    breaks: "`send` to a channel whose receiver is dropped is `Ok`",
    file: "src/runtime/channel.js",
    find: "  if (!channel.receiving) return { TAG: \"Err\", _0: [item] };\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "channels"],
  },
];
