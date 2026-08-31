// Negative (literal): a fully-literal eval argument is never captured, so it must
// NOT produce a code-injection candidate.
export function evaluateStatic(): unknown {
  return eval("1 + 1");
}
