// std's error of a message, which `Box<dyn Error>::from("..")` makes: a
// `dyn Error`'s dictionary, whose `value` is the message (ADR 0141).
function $stringError() {
  return {
    Debug: () => ({ fmt: (message) => $debugStr(message) }),
    Display: () => ({ fmt: (message) => message }),
    source: () => undefined,
  };
}
