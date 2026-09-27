# 0087. `println!` is `console.log`; `print!` writes as it is where JS can

Status: Accepted. Extends [0058](0058-format-options.md) and [0066](0066-template-literals.md).

## Context

`println!` is how a Rust program says what it's doing: a command-line tool's
output, a test's progress, a line of debugging. rustc's own tests print what
they check. It was an error: `std::io::_print` wasn't known.

JS has `console.log`, which writes a line, in Node, Bun and Deno to stdout
and in a browser to its console. It has nothing that writes part of a line,
except where there's a process: `process.stdout.write`.

## Decision

**A whole line is `console.log`'s, and `eprintln!`'s is `console.error`'s:**

```rust
println!("n = {n}, twice = {}", n * 2);
eprintln!("warn {:?}", v);
```

```js
console.log(`n = ${n}, twice = ${arg}`);
console.error(`warn [${v.map((item) => String(item)).join(", ")}]`);
```

The text is `format!`'s (ADR 0058), without the newline `println!` ends it
with; `console.log` ends the line. A `print!` of text that ends with one,
`print!("done\n")`, is the same call, so it's the same.

**Text that may not end a line, `print!("a")`, is `$print(text)`:** where
there's a `process`, as in Node, Bun and Deno, it's written as it is, to the
stream `console.log` writes to, so the output is byte for byte Rust's. In a
browser, whose console only writes lines, each line is written as it ends,
and what's left when the task ends.

A single string argument is written as it is: `console.log("100% %s")`
prints `100% %s`, in Node and Bun.

## Why

- **It's Rust's output.** The `print` case of the corpus (ADR 0088) writes
  lines, parts of lines, empty lines, stderr, `%` and escapes, and Bun and
  Node print exactly what native Rust prints, byte for byte.
- **It reads as JS would write it:** a `println!` is a `console.log`, not a
  call of a helper.

## Alternatives

- **Everything through one helper, as Emscripten and Kotlin/JS do:** right
  in a browser too, but every line would be `$println(..)`, not
  `console.log(..)`, for what only a browser's console gets wrong.
- **`process.stdout.write` for every line:** exact, but not in a browser.

## Consequences

- In a browser, a `println!` after a `print!` whose line hasn't ended is
  written first: `print!("a "); println!()` shows an empty line, then `a `
  when the task ends. Where there's a `process`, it's Rust's order.
- `std::io::stdout()` and `write!` to it are still errors.
