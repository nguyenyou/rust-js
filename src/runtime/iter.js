
function $iter(items) {
  return {
    items,
    at: 0,
    next() {
      return this.at < this.items.length ? { value: this.items[this.at++], done: false } : { value: undefined, done: true };
    },
    [Symbol.iterator]() {
      return this;
    },
  };
}
