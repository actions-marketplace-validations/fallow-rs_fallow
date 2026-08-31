// Positive: a recursive merge of a non-literal (attacker-shaped) source is a
// prototype-pollution candidate (CWE-1321). The merged source can carry
// `__proto__` / `constructor` keys.
import merge from "lodash.merge";

export function applyConfig(base: Record<string, unknown>, userInput: Record<string, unknown>): unknown {
  return merge(base, userInput);
}

// Positive: a direct static `obj.__proto__ = <non-literal>` member-assign writes
// the prototype directly. The callee path `target.__proto__` flattens cleanly (the
// object is a bare identifier) and matches the `*.__proto__` member-assign matcher.
export function setProto(target: { __proto__?: unknown }, evil: unknown): void {
  target.__proto__ = evil;
}
