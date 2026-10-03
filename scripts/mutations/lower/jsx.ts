// Mutations of src/lower/jsx.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "binding-component-as-value",
    breaks: "a JS module's component is lowered as a value, an arrow, which isn't a JSX tag",
    file: "src/lower/jsx.rs",
    find: "                let tag = match self.binding_component(component) {\n",
    replace: "                let tag = match self.binding_component(component).filter(|_| false) {\n",
    tests: ["test/jsx.test.ts", "-t", "a binding is a value"],
  },
];
