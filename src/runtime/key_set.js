
class $KeySet extends Set {
  #items = new Map();
  constructor(items) {
    super();
    if (items) {
      for (const item of items) {
        this.add(item);
      }
    }
  }
  get size() {
    return this.#items.size;
  }
  add(item) {
    const found = $key(item);
    if (!this.#items.has(found)) {
      this.#items.set(found, item);
    }
    return this;
  }
  has(item) {
    return this.#items.has($key(item));
  }
  delete(item) {
    return this.#items.delete($key(item));
  }
  clear() {
    this.#items.clear();
  }
  values() {
    return this.#items.values();
  }
  keys() {
    return this.#items.values();
  }
  *entries() {
    for (const item of this.#items.values()) {
      yield [item, item];
    }
  }
  [Symbol.iterator]() {
    return this.#items.values();
  }
  forEach(f, that) {
    for (const item of this.#items.values()) {
      f.call(that, item, item, this);
    }
  }
}
