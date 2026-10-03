// Mutations of src/format.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "transform-map-unmoved",
    breaks: "a transform's text is written with the map of the text it was given, which points at other lines",
    file: "src/format.rs",
    find: "    let map = moved(code, &text, map, &before, &after, js_file_name);\n    Ok((text, map))",
    replace: "    let _ = (&before, &after);\n    Ok((text, map.to_json_string()))",
    tests: ["test/settings.test.ts", "-t", "a transform's text is written"],
  },
  {
    name: "transform-layout-is-program",
    breaks: "Prettier's JSX text lines and `{\" \"}` count as another program, and its layout is refused",
    file: "src/format.rs",
    find: "        if !(space || spaces || kind == AstType::JSXText) {",
    replace: "        if true {",
    tests: ["test/settings.test.ts", "-t", "Prettier's layout"],
  },
  {
    name: "transform-unchecked",
    breaks: "a transform that takes out a statement is written, its map pointing at other code",
    file: "src/format.rs",
    find: "    if let Some(at) = (0..was.len().max(is.len())).find(|&i| was.get(i) != is.get(i)) {",
    replace: "    if let Some(at) = (0..0).find(|&i| was.get(i) != is.get(i)) {",
    tests: ["test/settings.test.ts", "-t", "is refused, and its input kept"],
  },
  {
    name: "format-width-ignored",
    breaks: "a crate's `printWidth` is ignored, and its JS laid out at 100 columns",
    file: "src/format.rs",
    find: "    if let Some(width) = format.print_width {",
    replace: "    if let Some(width) = format.print_width.filter(|_| false) {",
    tests: ["test/settings.test.ts", "-t", "formatter's options"],
  },
];
