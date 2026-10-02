
function $debugParseError(message, name) {
  const kinds = {
    "cannot parse integer from empty string": "Empty",
    "invalid digit found in string": "InvalidDigit",
    "number too large to fit in target type": "PosOverflow",
    "number too small to fit in target type": "NegOverflow",
    "number would be zero for non-zero type": "Zero",
    "cannot parse float from empty string": "Empty",
    "invalid float literal": "Invalid",
    "cannot parse char from empty string": "EmptyString",
    "too many characters in string": "TooManyChars",
  };
  if (name === "TryFromIntError") return `TryFromIntError(${message})`;
  return name === "ParseBoolError" ? name : `${name} { kind: ${kinds[message]} }`;
}
