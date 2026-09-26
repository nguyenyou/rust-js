# 0073. Named imports for generated modules

Status: Accepted. Refines 0019 and 0069.

## Context

Generated code should read like handwritten JavaScript and JSX. Namespace
imports made every component verbose: `<pane.Pane>` and `<toolbar.Toolbar>`.
Most exports have unambiguous names; collisions should be the exception.

## Decision

Import each used export by name, grouped into one declaration per module:

```jsx
import { Pane } from "./pane.jsx";
import { Toolbar } from "./toolbar.jsx";
```

JSX uses `<Pane>` and `<Toolbar>`. The same rule applies to functions,
constants, thread-local values and method objects (`Square.area(...)`).
External JavaScript bindings keep their explicitly requested import forms.

Lowering records a symbolic reference to the module and export. After
reachability, the linker reserves existing names, including nested parameters
and variable references, and assigns a local import name. Keep the export's
name when available; otherwise use an import alias such as `Pane as Pane$1`.
Sort by module path and export name so collision handling is deterministic.
Import a method's owning object once, even when several methods are used.

Keep ES module live bindings, relative paths, dependency manifests and source
spans. No textual replacement of generated JavaScript is involved.

The playground preview uses an import map with data URLs and virtual module
specifiers. It rewrites import specifiers, then lets the browser link the
modules, including cycles and live bindings. Each URL includes the module's
path so identical source text still has independent module state. This
replaces the classic-script preview linker, which only understood namespace
imports. Module loading and async `main` failures reach the existing report UI.

## Validation

Tests cover unambiguous JSX, duplicate component exports, local and nested
name collisions, source maps, grouped imports, methods, constants, thread-local
values, cyclic modules and deterministic output. Playground snapshots and
committed generated files show the new output; browser tests cover native/WASM
parity and React Fast Refresh.
