// What JS holds for the harness's own values in test/native.rs, called as
// generated JS is: if these match, the oracle's encoding, from native Rust
// and back from Node (node-calls.ts), loses nothing.

export const escapes = () => 'tab\t nul\0 esc\u001b "quoted" back\\slash é';
export const floats = () => [-0, NaN, Infinity, -Infinity, 0.1 + 0.2];
export const bigint = () => 18446744073709551615n;
export const panic = () => {
  throw new Error('a "quoted"\nmessage');
};
