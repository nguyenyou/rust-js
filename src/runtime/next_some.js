
function $nextSome(it) {
  const step = it.next();
  return step.done ? undefined : $some(step.value);
}
