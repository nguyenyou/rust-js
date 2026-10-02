
function $debugFields(name, fields, values, alternate = false) {
  const shown = fields.map((field, i) => field + ": " + values[i]);
  return alternate ? $pretty(name + " {", shown, "}") : name + " { " + shown.join(", ") + " }";
}
