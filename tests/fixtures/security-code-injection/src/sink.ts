// Positive: a non-literal value passed to eval() is a code-injection candidate
// (CWE-94). `eval` has no provenance gate (it is a global).
export function evaluate(userInput: string): unknown {
  return eval(userInput);
}
