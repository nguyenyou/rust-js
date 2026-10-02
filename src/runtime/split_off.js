
function $splitOff(items, at) {
  if (at > items.length) throw new Error(`\`at\` split index (is ${at}) should be <= len (is ${items.length})`);
  return items.splice(at);
}
