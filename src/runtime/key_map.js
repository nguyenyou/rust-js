
class $KeyMap extends Map {
  constructor(entries) {
    super();
    if (entries) {
      for (const [key, value] of entries) {
        this.set(key, value);
      }
    }
  }
  get(key) {
    const entry = super.get($key(key));
    return entry === undefined ? undefined : entry[1];
  }
  set(key, value) {
    const found = $key(key);
    const entry = super.get(found);
    if (entry === undefined) {
      super.set(found, [key, value]);
    } else {
      entry[1] = value;
    }
    return this;
  }
  has(key) {
    return super.has($key(key));
  }
  delete(key) {
    return super.delete($key(key));
  }
  *entries() {
    for (const [key, value] of super.values()) {
      yield [key, value];
    }
  }
  *keys() {
    for (const [key] of super.values()) {
      yield key;
    }
  }
  *values() {
    for (const [, value] of super.values()) {
      yield value;
    }
  }
  [Symbol.iterator]() {
    return this.entries();
  }
  forEach(f, that) {
    for (const [key, value] of this.entries()) {
      f.call(that, value, key, this);
    }
  }
}
