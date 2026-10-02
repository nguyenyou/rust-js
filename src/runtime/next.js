
function $next(it) {
  const step = it.next();
  return step.done ? undefined : step.value;
}
