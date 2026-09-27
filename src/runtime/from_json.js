
// serde_json's reader (ADR 0078): its `Deserializer`, ported step for step,
// with its methods' names. It reads the text's UTF-8 bytes, as serde_json
// does, so a mistake is found at the same byte, with the same message, and
// its column counts bytes.
class $JsonReader {
  constructor(text) {
    this.bytes = new TextEncoder().encode(text);
    this.index = 0;
    this.remainingDepth = 128;
  }

  // The next byte, or -1 at the end.
  peek() {
    return this.index < this.bytes.length ? this.bytes[this.index] : -1;
  }

  next() {
    return this.index < this.bytes.length ? this.bytes[this.index++] : -1;
  }

  // `position_of_index`: the line, and the bytes before `index` on it.
  errorAt(index, message) {
    const start = index === 0 ? 0 : this.bytes.lastIndexOf(10, index - 1) + 1;
    let line = 1;
    for (let i = 0; i < start; i++) if (this.bytes[i] === 10) line++;
    return new $JsonError(message, line, index - start);
  }

  error(message) {
    return this.errorAt(this.index, message);
  }

  peekError(message) {
    return this.errorAt(Math.min(this.bytes.length, this.index + 1), message);
  }

  // `fix_position`: a visitor's message has no place, until it gets where
  // the reader is.
  fixPosition(e) {
    return e instanceof $JsonError && e.line === 0 ? this.error(e.message) : e;
  }

  end() {
    if (this.parseWhitespace() !== -1) throw this.peekError("trailing characters");
  }

  parseWhitespace() {
    for (;;) {
      const c = this.peek();
      if (c !== 32 && c !== 10 && c !== 9 && c !== 13) return c;
      this.index++;
    }
  }

  parseIdent(rest) {
    for (let i = 0; i < rest.length; i++) {
      const c = this.next();
      if (c === -1) throw this.error("EOF while parsing a value");
      if (c !== rest.charCodeAt(i)) throw this.error("expected ident");
    }
  }

  // What's there instead of `expected`, read past, in serde's words.
  peekInvalidType(expected) {
    const c = this.peek();
    let unexpected;
    if (c === 110) {
      this.index++;
      this.parseIdent("ull");
      unexpected = "null";
    } else if (c === 116) {
      this.index++;
      this.parseIdent("rue");
      unexpected = "boolean `true`";
    } else if (c === 102) {
      this.index++;
      this.parseIdent("alse");
      unexpected = "boolean `false`";
    } else if (c === 45) {
      this.index++;
      unexpected = $jsonUnexpectedNumber(this.parseInteger(false));
    } else if (c >= 48 && c <= 57) {
      unexpected = $jsonUnexpectedNumber(this.parseInteger(true));
    } else if (c === 34) {
      this.index++;
      unexpected = `string ${$debugStr(this.parseStr())}`;
    } else if (c === 91) {
      unexpected = "sequence";
    } else if (c === 123) {
      unexpected = "map";
    } else {
      return this.peekError("expected value");
    }
    return this.fixPosition($jsonError(`invalid type: ${unexpected}, expected ${expected}`));
  }

  // A number: `{ kind, value }`, a `"u"`nsigned or `"i"`nteger (a BigInt
  // past 2^53) where it's an integer that fits in 64 bits, else an `"f"`.
  deserializeNumber(expected, visit) {
    const peek = this.parseWhitespace();
    if (peek === -1) throw this.peekError("EOF while parsing a value");
    try {
      if (peek === 45) {
        this.index++;
        return visit(this.parseInteger(false));
      }
      if (peek >= 48 && peek <= 57) return visit(this.parseInteger(true));
      throw this.peekInvalidType(expected);
    } catch (e) {
      throw this.fixPosition(e);
    }
  }

  parseInteger(positive) {
    const c = this.next();
    if (c === -1) throw this.error("EOF while parsing a value");
    if (c === 48) {
      const p = this.peek();
      if (p >= 48 && p <= 57) throw this.peekError("invalid number");
      return this.parseNumber(positive, 0);
    }
    if (c < 49 || c > 57) throw this.error("invalid number");
    let significand = c - 48;
    for (;;) {
      const p = this.peek();
      if (p < 48 || p > 57) return this.parseNumber(positive, significand);
      const grown = $jsonGrow(significand, p - 48);
      if (grown > 18446744073709551615n) {
        return { kind: "f", value: this.parseLongInteger(positive, significand) };
      }
      this.index++;
      significand = grown;
    }
  }

  parseNumber(positive, significand) {
    const p = this.peek();
    if (p === 46) return { kind: "f", value: this.parseDecimal(positive, significand, 0) };
    if (p === 101 || p === 69) return { kind: "f", value: this.parseExponent(positive, significand, 0) };
    if (positive) return { kind: "u", value: significand };
    // `-0`, and below `i64::MIN`, are floats.
    if (significand === 0 || significand > 9223372036854775808n) {
      return { kind: "f", value: -Number(significand) };
    }
    return { kind: "i", value: -significand };
  }

  parseDecimal(positive, significand, exponentBeforeDecimalPoint) {
    this.index++;
    let exponentAfterDecimalPoint = 0;
    for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) {
      const grown = $jsonGrow(significand, p - 48);
      if (grown > 18446744073709551615n) {
        const exponent = exponentBeforeDecimalPoint + exponentAfterDecimalPoint;
        return this.parseDecimalOverflow(positive, significand, exponent);
      }
      this.index++;
      significand = grown;
      exponentAfterDecimalPoint--;
    }
    if (exponentAfterDecimalPoint === 0) {
      throw this.peekError(this.peek() === -1 ? "EOF while parsing a value" : "invalid number");
    }
    const exponent = exponentBeforeDecimalPoint + exponentAfterDecimalPoint;
    const p = this.peek();
    return p === 101 || p === 69
      ? this.parseExponent(positive, significand, exponent)
      : this.f64FromParts(positive, significand, exponent);
  }

  parseExponent(positive, significand, startingExp) {
    this.index++;
    let positiveExp = true;
    const sign = this.peek();
    if (sign === 43) {
      this.index++;
    } else if (sign === 45) {
      this.index++;
      positiveExp = false;
    }
    const c = this.next();
    if (c === -1) throw this.error("EOF while parsing a value");
    if (c < 48 || c > 57) throw this.error("invalid number");
    let exp = c - 48;
    for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) {
      this.index++;
      if (exp * 10 + (p - 48) > 2147483647) {
        return this.parseExponentOverflow(positive, significand == 0, positiveExp);
      }
      exp = exp * 10 + (p - 48);
    }
    // i32's saturating add and subtract.
    const finalExp = Math.max(-2147483648, Math.min(2147483647, positiveExp ? startingExp + exp : startingExp - exp));
    return this.f64FromParts(positive, significand, finalExp);
  }

  // Not correctly rounded, as serde_json's isn't without its
  // `float_roundtrip` feature: the significand times or over a power of ten.
  f64FromParts(positive, significand, exponent) {
    let f = Number(significand);
    for (;;) {
      const pow = $JSON_POW10[Math.abs(exponent)];
      if (pow !== undefined) {
        if (exponent >= 0) {
          f *= pow;
          if (!Number.isFinite(f)) throw this.error("number out of range");
        } else {
          f /= pow;
        }
        break;
      }
      if (f === 0) break;
      if (exponent >= 0) throw this.error("number out of range");
      f /= 1e308;
      exponent += 308;
    }
    return positive ? f : -f;
  }

  parseLongInteger(positive, significand) {
    let exponent = 0;
    for (;;) {
      const p = this.peek();
      if (p >= 48 && p <= 57) {
        this.index++;
        exponent++;
      } else if (p === 46) {
        return this.parseDecimal(positive, significand, exponent);
      } else if (p === 101 || p === 69) {
        return this.parseExponent(positive, significand, exponent);
      } else {
        return this.f64FromParts(positive, significand, exponent);
      }
    }
  }

  parseDecimalOverflow(positive, significand, exponent) {
    for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) this.index++;
    const p = this.peek();
    return p === 101 || p === 69
      ? this.parseExponent(positive, significand, exponent)
      : this.f64FromParts(positive, significand, exponent);
  }

  parseExponentOverflow(positive, zeroSignificand, positiveExp) {
    if (!zeroSignificand && positiveExp) throw this.error("number out of range");
    for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) this.index++;
    return positive ? 0 : -0;
  }

  // A string, after its opening quote.
  parseStr() {
    const bytes = this.bytes;
    let text = "";
    let start = this.index;
    for (;;) {
      while (this.index < bytes.length) {
        const c = bytes[this.index];
        if (c === 34 || c === 92 || c < 32) break;
        this.index++;
      }
      if (this.index === bytes.length) throw this.error("EOF while parsing a string");
      const c = bytes[this.index];
      if (c === 34) {
        text += $JSON_UTF8.decode(bytes.subarray(start, this.index));
        this.index++;
        return text;
      }
      if (c !== 92) {
        this.index++;
        throw this.error("control character (\\u0000-\\u001F) found while parsing a string");
      }
      text += $JSON_UTF8.decode(bytes.subarray(start, this.index));
      this.index++;
      text += this.parseEscape();
      start = this.index;
    }
  }

  parseEscape() {
    const c = this.next();
    if (c === -1) throw this.error("EOF while parsing a string");
    switch (c) {
      case 34:
        return '"';
      case 92:
        return "\\";
      case 47:
        return "/";
      case 98:
        return "\b";
      case 102:
        return "\f";
      case 110:
        return "\n";
      case 114:
        return "\r";
      case 116:
        return "\t";
      case 117:
        return this.parseUnicodeEscape();
    }
    throw this.error("invalid escape");
  }

  parseUnicodeEscape() {
    const n = this.decodeHexEscape();
    // A trailing surrogate, which serde_json calls a leading one.
    if (n >= 0xdc00 && n <= 0xdfff) throw this.error("lone leading surrogate in hex escape");
    if (n < 0xd800 || n > 0xdbff) return String.fromCharCode(n);
    for (const expected of [92, 117]) {
      if (this.peek() === -1) throw this.error("EOF while parsing a string");
      const c = this.bytes[this.index++];
      if (c !== expected) throw this.error("unexpected end of hex escape");
    }
    const n2 = this.decodeHexEscape();
    if (n2 < 0xdc00 || n2 > 0xdfff) throw this.error("lone leading surrogate in hex escape");
    return String.fromCharCode(n, n2);
  }

  decodeHexEscape() {
    if (this.index + 4 > this.bytes.length) {
      this.index = this.bytes.length;
      throw this.error("EOF while parsing a string");
    }
    let n = 0;
    let valid = true;
    for (let i = 0; i < 4; i++) {
      const digit = $jsonHexDigit(this.bytes[this.index + i]);
      valid &&= digit >= 0;
      n = n * 16 + digit;
    }
    this.index += 4;
    if (!valid) throw this.error("invalid escape");
    return n;
  }

  ignoreStr() {
    const bytes = this.bytes;
    for (;;) {
      while (this.index < bytes.length) {
        const c = bytes[this.index];
        if (c === 34 || c === 92 || c < 32) break;
        this.index++;
      }
      if (this.index === bytes.length) throw this.error("EOF while parsing a string");
      const c = bytes[this.index];
      if (c === 34) {
        this.index++;
        return;
      }
      if (c !== 92) throw this.error("control character (\\u0000-\\u001F) found while parsing a string");
      this.index++;
      const escape = this.next();
      if (escape === -1) throw this.error("EOF while parsing a string");
      if (escape === 117) this.decodeHexEscape();
      else if (!'"\\/bfnrt'.includes(String.fromCharCode(escape))) throw this.error("invalid escape");
    }
  }

  parseObjectColon() {
    const c = this.parseWhitespace();
    if (c === 58) {
      this.index++;
      return;
    }
    throw this.peekError(c === -1 ? "EOF while parsing an object" : "expected `:`");
  }

  endSeq() {
    const c = this.parseWhitespace();
    if (c === 93) {
      this.index++;
      return;
    }
    if (c === 44) {
      this.index++;
      throw this.peekError(this.parseWhitespace() === 93 ? "trailing comma" : "trailing characters");
    }
    throw this.peekError(c === -1 ? "EOF while parsing a list" : "trailing characters");
  }

  endMap() {
    const c = this.parseWhitespace();
    if (c === 125) {
      this.index++;
      return;
    }
    throw this.peekError(c === 44 ? "trailing comma" : c === -1 ? "EOF while parsing an object" : "trailing characters");
  }

  ignoreValue() {
    const frames = [];
    let enclosing;
    for (;;) {
      const peek = this.parseWhitespace();
      if (peek === -1) throw this.peekError("EOF while parsing a value");
      let frame;
      if (peek === 110) {
        this.index++;
        this.parseIdent("ull");
      } else if (peek === 116) {
        this.index++;
        this.parseIdent("rue");
      } else if (peek === 102) {
        this.index++;
        this.parseIdent("alse");
      } else if (peek === 45) {
        this.index++;
        this.ignoreInteger();
      } else if (peek >= 48 && peek <= 57) {
        this.ignoreInteger();
      } else if (peek === 34) {
        this.index++;
        this.ignoreStr();
      } else if (peek === 91 || peek === 123) {
        if (enclosing !== undefined) frames.push(enclosing);
        enclosing = undefined;
        this.index++;
        frame = peek;
      } else {
        throw this.peekError("expected value");
      }
      let acceptComma = true;
      if (frame !== undefined) {
        acceptComma = false;
      } else if (enclosing !== undefined) {
        frame = enclosing;
        enclosing = undefined;
      } else if (frames.length > 0) {
        frame = frames.pop();
      } else {
        return;
      }
      for (;;) {
        const c = this.parseWhitespace();
        if (c === 44 && acceptComma) {
          this.index++;
          break;
        }
        if (c === -1) throw this.peekError(frame === 91 ? "EOF while parsing a list" : "EOF while parsing an object");
        if (!((c === 93 && frame === 91) || (c === 125 && frame === 123))) {
          if (acceptComma) throw this.peekError(frame === 91 ? "expected `,` or `]`" : "expected `,` or `}`");
          break;
        }
        this.index++;
        if (frames.length === 0) return;
        frame = frames.pop();
        acceptComma = true;
      }
      if (frame === 123) {
        const quote = this.parseWhitespace();
        if (quote !== 34) throw this.peekError(quote === -1 ? "EOF while parsing an object" : "key must be a string");
        this.index++;
        this.ignoreStr();
        const colon = this.parseWhitespace();
        if (colon !== 58) throw this.peekError(colon === -1 ? "EOF while parsing an object" : "expected `:`");
        this.index++;
      }
      enclosing = frame;
    }
  }

  ignoreInteger() {
    const c = this.next();
    if (c === 48) {
      const p = this.peek();
      if (p >= 48 && p <= 57) throw this.peekError("invalid number");
    } else if (c >= 49 && c <= 57) {
      for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) this.index++;
    } else {
      throw this.error("invalid number");
    }
    const p = this.peek();
    if (p === 46) this.ignoreDecimal();
    else if (p === 101 || p === 69) this.ignoreExponent();
  }

  ignoreDecimal() {
    this.index++;
    let atLeastOneDigit = false;
    for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) {
      this.index++;
      atLeastOneDigit = true;
    }
    if (!atLeastOneDigit) throw this.peekError("invalid number");
    const p = this.peek();
    if (p === 101 || p === 69) this.ignoreExponent();
  }

  ignoreExponent() {
    this.index++;
    const sign = this.peek();
    if (sign === 43 || sign === 45) this.index++;
    const c = this.next();
    if (c < 48 || c > 57) throw this.error("invalid number");
    for (let p = this.peek(); p >= 48 && p <= 57; p = this.peek()) this.index++;
  }

  // An array or an object: `visit` reads what's inside, and the closing
  // bracket is looked for even when it fails, as serde_json does.
  nested(visit, end) {
    if (--this.remainingDepth === 0) throw this.peekError("recursion limit exceeded");
    this.index++;
    let value;
    let failure;
    try {
      value = visit();
    } catch (e) {
      if (!(e instanceof $JsonError)) throw e;
      failure = e;
    }
    this.remainingDepth++;
    try {
      end();
    } catch (e) {
      if (!(e instanceof $JsonError)) throw e;
      failure ??= e;
    }
    if (failure) throw failure;
    return value;
  }

  // `deserialize_seq`: `[ .. ]`, which `visit` reads with a `$JsonSeq`.
  deserializeSeq(expected, visit) {
    const peek = this.parseWhitespace();
    if (peek === -1) throw this.peekError("EOF while parsing a value");
    try {
      if (peek !== 91) throw this.peekInvalidType(expected);
      return this.nested(() => visit(new $JsonSeq(this)), () => this.endSeq());
    } catch (e) {
      throw this.fixPosition(e);
    }
  }

  // `deserialize_map` and `deserialize_struct`: `{ .. }`, or for a struct,
  // `[ .. ]` too.
  deserializeMap(expected, visitMap, visitSeq) {
    const peek = this.parseWhitespace();
    if (peek === -1) throw this.peekError("EOF while parsing a value");
    try {
      if (peek === 91 && visitSeq) return this.nested(() => visitSeq(new $JsonSeq(this)), () => this.endSeq());
      if (peek !== 123) throw this.peekInvalidType(expected);
      return this.nested(() => visitMap(new $JsonMap(this)), () => this.endMap());
    } catch (e) {
      throw this.fixPosition(e);
    }
  }

  deserializeStr(expected, visit) {
    const peek = this.parseWhitespace();
    if (peek === -1) throw this.peekError("EOF while parsing a value");
    try {
      if (peek !== 34) throw this.peekInvalidType(expected);
      this.index++;
      return visit(this.parseStr());
    } catch (e) {
      throw this.fixPosition(e);
    }
  }

  // What serde's own impls read.

  bool() {
    const peek = this.parseWhitespace();
    if (peek === -1) throw this.peekError("EOF while parsing a value");
    try {
      if (peek === 116) {
        this.index++;
        this.parseIdent("rue");
        return true;
      }
      if (peek === 102) {
        this.index++;
        this.parseIdent("alse");
        return false;
      }
      throw this.peekInvalidType("a boolean");
    } catch (e) {
      throw this.fixPosition(e);
    }
  }

  // An integer type, named as serde names it, from `min` to `max`.
  int(name, min, max) {
    return this.deserializeNumber(name, (n) => {
      if (n.kind === "f") throw $jsonError(`invalid type: floating point \`${$jsonNumber(n.value)}\`, expected ${name}`);
      if (n.kind === "u" ? n.value > max : n.value < min) {
        throw $jsonError(`invalid value: integer \`${n.value}\`, expected ${name}`);
      }
      return Number(n.value);
    });
  }

  f64() {
    return this.deserializeNumber("f64", (n) => Number(n.value));
  }

  string() {
    return this.deserializeStr("a string", (s) => s);
  }

  char() {
    return this.deserializeStr("a character", $jsonChar);
  }

  unit(expected = "unit") {
    const peek = this.parseWhitespace();
    if (peek === -1) throw this.peekError("EOF while parsing a value");
    try {
      if (peek !== 110) throw this.peekInvalidType(expected);
      this.index++;
      this.parseIdent("ull");
      return undefined;
    } catch (e) {
      throw this.fixPosition(e);
    }
  }

  option(read) {
    if (this.parseWhitespace() !== 110) return read(this);
    this.index++;
    this.parseIdent("ull");
    return undefined;
  }

  vec(read) {
    return this.deserializeSeq("a sequence", (seq) => {
      const items = [];
      while (seq.next()) items.push(read(this));
      return items;
    });
  }

  tuple(reads) {
    const expected = `a tuple of size ${reads.length}`;
    return this.deserializeSeq(expected, (seq) => reads.map((read, i) => seq.element(read, i, expected)));
  }

  array(length, read) {
    const expected = length === 0 ? "an empty array" : `an array of length ${length}`;
    return this.deserializeSeq(expected, (seq) =>
      Array.from({ length }, (_, i) => seq.element(read, i, expected)),
    );
  }

  map(readKey, read) {
    return this.deserializeMap("a map", (map) => {
      const entries = new Map();
      while (map.next()) {
        const key = readKey(new $JsonKey(this));
        entries.set(key, map.value(read));
      }
      return entries;
    });
  }

  // What serde's derives read.

  // A struct's fields: `[name, read]`, or `[[name, ...aliases], read]`, and
  // `missing`, the value it has when it's not there, for `#[serde(default)]`.
  // `build` makes the struct of their values.
  struct(expected, fields, build, { deny = false, container, expecting } = {}) {
    const length = expecting ?? `${expected} with ${fields.length} element${fields.length === 1 ? "" : "s"}`;
    return this.deserializeMap(
      expecting ?? expected,
      (map) => {
        const values = new Array(fields.length);
        const seen = new Array(fields.length).fill(false);
        while (map.next()) {
          const i = $jsonField(new $JsonKey(this).string(), fields, deny);
          if (i < 0) {
            map.value((json) => json.ignoreValue());
            continue;
          }
          if (seen[i]) throw $jsonError(`duplicate field \`${$jsonName(fields[i][0])}\``);
          values[i] = map.value(fields[i][1]);
          seen[i] = true;
        }
        const defaults = container?.();
        fields.forEach(([names, read, missing], i) => {
          if (!seen[i]) values[i] = missing ? missing(defaults) : read(new $JsonMissing($jsonName(names)));
        });
        return build(values, defaults);
      },
      (seq) => {
        const defaults = container?.();
        const values = fields.map(([, read, missing], i) => seq.element(read, i, length, missing, defaults));
        return build(values, defaults);
      },
    );
  }

  // A tuple struct's fields: `read`, or `[read, missing]`.
  tupleStruct(expected, fields, build = (values) => values, { container, expecting } = {}) {
    const length = expecting ?? `${expected} with ${fields.length} element${fields.length === 1 ? "" : "s"}`;
    return this.deserializeSeq(expecting ?? expected, (seq) => {
      const defaults = container?.();
      const values = fields.map((field, i) => {
        const [read, missing] = typeof field === "function" ? [field] : field;
        return seq.element(read, i, length, missing, defaults);
      });
      return build(values, defaults);
    });
  }

  // `deserialize_enum`: `"Name"`, or `{"Name": ..}`. `visit` gets the
  // variant's name and a `$JsonVariant` to read what it holds.
  enum(variants, visit, other) {
    const peek = this.parseWhitespace();
    if (peek === 34) return visit(this.variant(variants, other), new $JsonVariant(this, true));
    if (peek !== 123) throw this.peekError(peek === -1 ? "EOF while parsing a value" : "expected value");
    if (--this.remainingDepth === 0) throw this.peekError("recursion limit exceeded");
    this.index++;
    let value;
    try {
      const c = this.parseWhitespace();
      if (c !== 34) {
        throw this.peekError(
          c === 125 ? "expected value" : c === -1 ? "EOF while parsing an object" : "key must be a string",
        );
      }
      const name = this.variant(variants, other);
      this.parseObjectColon();
      value = visit(name, new $JsonVariant(this, false));
    } finally {
      this.remainingDepth++;
    }
    const c = this.parseWhitespace();
    if (c === 125) {
      this.index++;
      return value;
    }
    throw this.error(c === -1 ? "EOF while parsing an object" : "expected value");
  }

  // A variant's name, from the reader at its opening quote.
  variant(variants, other) {
    try {
      this.index++;
      const name = this.parseStr();
      for (const names of variants) {
        if (typeof names === "string" ? names === name : names.includes(name)) return $jsonName(names);
      }
      if (other !== undefined) return other;
      throw variants.length === 0
        ? $jsonError(`unknown variant \`${name}\`, there are no variants`)
        : $jsonError(`unknown variant \`${name}\`, expected ${$jsonOneOf(variants)}`);
    } catch (e) {
      throw this.fixPosition(e);
    }
  }
}

// `SeqAccess`.
class $JsonSeq {
  constructor(reader) {
    this.reader = reader;
    this.first = true;
  }

  // `has_next_element`.
  next() {
    const reader = this.reader;
    const peek = reader.parseWhitespace();
    if (peek === -1) throw reader.peekError("EOF while parsing a list");
    if (peek === 93) return false;
    if (this.first) {
      this.first = false;
      return true;
    }
    if (peek !== 44) throw reader.peekError("expected `,` or `]`");
    reader.index++;
    const c = reader.parseWhitespace();
    if (c === 93) throw reader.peekError("trailing comma");
    if (c === -1) throw reader.peekError("EOF while parsing a value");
    return true;
  }

  // The `i`th of `expected`'s items.
  element(read, i, expected, missing, defaults) {
    if (this.next()) return read(this.reader);
    if (missing) return missing(defaults);
    throw $jsonError(`invalid length ${i}, expected ${expected}`);
  }
}

// `MapAccess`.
class $JsonMap {
  constructor(reader) {
    this.reader = reader;
    this.first = true;
  }

  // `has_next_key`.
  next() {
    const reader = this.reader;
    const peek = reader.parseWhitespace();
    if (peek === -1) throw reader.peekError("EOF while parsing an object");
    if (peek === 125) return false;
    if (this.first) {
      this.first = false;
      if (peek === 34) return true;
      throw reader.peekError("key must be a string");
    }
    if (peek !== 44) throw reader.peekError("expected `,` or `}`");
    reader.index++;
    const c = reader.parseWhitespace();
    if (c === 34) return true;
    throw reader.peekError(
      c === 125 ? "trailing comma" : c === -1 ? "EOF while parsing a value" : "key must be a string",
    );
  }

  value(read) {
    this.reader.parseObjectColon();
    return read(this.reader);
  }
}

// `MapKey`: an object's key, from its opening quote. A number or a `bool`
// is read from inside the quotes.
class $JsonKey {
  constructor(reader) {
    this.reader = reader;
  }

  string() {
    this.reader.index++;
    return this.reader.parseStr();
  }

  char() {
    return $jsonChar(this.string());
  }

  number(read) {
    const reader = this.reader;
    reader.index++;
    const c = reader.peek();
    if (!(c === 45 || (c >= 48 && c <= 57))) throw reader.error("invalid value: expected key to be a number in quotes");
    const value = read(reader);
    if (reader.peek() !== 34) throw reader.peekError('expected `"`');
    reader.index++;
    return value;
  }

  bool() {
    const reader = this.reader;
    reader.index++;
    try {
      const c = reader.next();
      if (c === -1) throw reader.peekError("EOF while parsing a value");
      if (c === 116) {
        reader.parseIdent('rue"');
        return true;
      }
      if (c === 102) {
        reader.parseIdent('alse"');
        return false;
      }
      throw $jsonError(`invalid type: string ${$debugStr(reader.parseStr())}, expected a boolean`);
    } catch (e) {
      throw reader.fixPosition(e);
    }
  }
}

// `VariantAccess` of `{"Name": ..}`, or of `"Name"` (`unit`), which holds
// nothing.
class $JsonVariant {
  constructor(reader, unit) {
    this.reader = reader;
    this.isUnit = unit;
  }

  unit() {
    if (!this.isUnit) this.reader.unit();
  }

  newtype(read) {
    if (this.isUnit) throw $jsonError("invalid type: unit variant, expected newtype variant");
    return read(this.reader);
  }

  tuple(expected, fields, build, options) {
    if (this.isUnit) throw $jsonError("invalid type: unit variant, expected tuple variant");
    return this.reader.tupleStruct(expected, fields, build, options);
  }

  struct(expected, fields, build, options) {
    if (this.isUnit) throw $jsonError("invalid type: unit variant, expected struct variant");
    return this.reader.struct(expected, fields, build, options);
  }
}

// serde's `missing_field`: what a field that isn't there reads as, which is
// `None` for an `Option`, and otherwise an error.
class $JsonMissing {
  constructor(name) {
    this.name = name;
  }

  option() {
    return undefined;
  }
}
for (const method of ["bool", "int", "f64", "string", "char", "unit", "vec", "tuple", "array", "map", "struct", "tupleStruct", "enum"]) {
  $JsonMissing.prototype[method] = function () {
    throw $jsonError(`missing field \`${this.name}\``);
  };
}

const $JSON_UTF8 = new TextDecoder();
const $JSON_POW10 = Array.from({ length: 309 }, (_, i) => Number(`1e${i}`));

// `significand * 10 + digit`, a BigInt once it's past 2^53.
function $jsonGrow(significand, digit) {
  if (typeof significand === "number") {
    const grown = significand * 10 + digit;
    if (grown <= Number.MAX_SAFE_INTEGER) return grown;
  }
  return BigInt(significand) * 10n + BigInt(digit);
}

function $jsonHexDigit(c) {
  if (c >= 48 && c <= 57) return c - 48;
  if (c >= 65 && c <= 70) return c - 55;
  if (c >= 97 && c <= 102) return c - 87;
  return -1;
}

// serde's `Unexpected` of a number, a float as serde_json writes one.
function $jsonUnexpectedNumber(n) {
  return n.kind === "f" ? `floating point \`${$jsonNumber(n.value)}\`` : `integer \`${n.value}\``;
}

function $jsonChar(s) {
  if ([...s].length === 1) return s;
  throw $jsonError(`invalid value: string ${$debugStr(s)}, expected a character`);
}

function $jsonName(names) {
  return typeof names === "string" ? names : names[0];
}

// The field a key names, or -1 to skip its value.
function $jsonField(key, fields, deny) {
  const i = fields.findIndex(([names]) => (typeof names === "string" ? names === key : names.includes(key)));
  if (i >= 0 || !deny) return i;
  throw fields.length === 0
    ? $jsonError(`unknown field \`${key}\`, there are no fields`)
    : $jsonError(`unknown field \`${key}\`, expected ${$jsonOneOf(fields.map(([names]) => names))}`);
}

// serde's `OneOf`, of each name and its aliases, which serde keeps sorted.
function $jsonOneOf(entries) {
  const byCodePoint = (a, b) => {
    const [x, y] = [[...a], [...b]];
    for (let i = 0; i < Math.min(x.length, y.length); i++) {
      const d = x[i].codePointAt(0) - y[i].codePointAt(0);
      if (d !== 0) return d;
    }
    return x.length - y.length;
  };
  const names = entries.flatMap((names) => (typeof names === "string" ? [names] : [...names].sort(byCodePoint)));
  const quoted = names.map((name) => `\`${name}\``);
  if (quoted.length === 1) return quoted[0];
  if (quoted.length === 2) return `${quoted[0]} or ${quoted[1]}`;
  return `one of ${quoted.join(", ")}`;
}

// What the crate reads with: `$json.u32`, `$json.option($json.string)`.
const $json = {
  bool: (json) => json.bool(),
  u8: (json) => json.int("u8", 0, 255),
  u16: (json) => json.int("u16", 0, 65535),
  u32: (json) => json.int("u32", 0, 4294967295),
  usize: (json) => json.int("usize", 0, 4294967295),
  i8: (json) => json.int("i8", -128, 127),
  i16: (json) => json.int("i16", -32768, 32767),
  i32: (json) => json.int("i32", -2147483648, 2147483647),
  isize: (json) => json.int("isize", -2147483648, 2147483647),
  f64: (json) => json.f64(),
  string: (json) => json.string(),
  char: (json) => json.char(),
  unit: (json) => json.unit(),
  option: (read) => (json) => json.option(read),
  vec: (read) => (json) => json.vec(read),
  set: (read) => (json) => new Set(json.vec(read)),
  map: (readKey, read) => (json) => json.map(readKey, read),
  tuple: (...reads) => (json) => json.tuple(reads),
  array: (length, read) => (json) => json.array(length, read),
  // An object's keys.
  key: {
    string: (key) => key.string(),
    char: (key) => key.char(),
    bool: (key) => key.bool(),
    number: (read) => (key) => key.number(read),
  },
};

// `serde_json::from_str`: a `Result` of what `read` reads, which must be
// all there is.
function $fromJson(text, read) {
  const json = new $JsonReader(text);
  try {
    const value = read(json);
    json.end();
    return { TAG: "Ok", _0: value };
  } catch (e) {
    if (e instanceof $JsonError) return { TAG: "Err", _0: { message: e.message, line: e.line, column: e.column } };
    throw e;
  }
}
