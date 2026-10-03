// Mutations of src/lower/bindings.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "camel-case-unread",
    breaks: "`js::camel_case!();` names nothing the JS way",
    file: "src/lower/bindings.rs",
    find: "    marks(tcx, LocalModDefId::CRATE_DEF_ID, \"camel_case\").next().is_some()\n",
    replace: "    marks(tcx, LocalModDefId::CRATE_DEF_ID, \"none\").next().is_some()\n",
    tests: ["test/react.test.ts","-t","camel_case crate"],
  },
];
