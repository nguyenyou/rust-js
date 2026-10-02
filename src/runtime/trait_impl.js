
function $traitImpl(cache, keys, make) {
  // Each key is a dictionary or a drop, held weakly: a drop made for one call
  // goes when it does. No drop, `undefined`, is keyed by this function. A
  // const parameter's value is a number or the like, which only a Map holds.
  for (let i = 0; i < keys.length - 1; i++) {
    const key = keys[i] ?? $traitImpl;
    const next = keys[i + 1] ?? $traitImpl;
    const weak = typeof next === "object" || typeof next === "function";
    if (!cache.has(key)) cache.set(key, weak ? new WeakMap() : new Map());
    cache = cache.get(key);
  }
  const key = keys[keys.length - 1] ?? $traitImpl;
  if (!cache.has(key)) cache.set(key, make());
  return cache.get(key);
}
