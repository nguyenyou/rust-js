
function $key(value) {
  if (value == null) {
    return "~";
  }
  switch (typeof value) {
    case "string":
      return JSON.stringify(value);
    case "bigint":
      return value + "n";
    case "object":
      if (Array.isArray(value)) {
        return "[" + value.map($key).join(",") + "]";
      }
      return "{" + Object.keys(value).sort().map((k) => JSON.stringify(k) + ":" + $key(value[k])).join(",") + "}";
    default:
      return String(value);
  }
}
