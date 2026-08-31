// Negative (literal): merging a fully-literal object source is never captured
// (the source argument is literal), so it must NOT produce a candidate.
import merge from "lodash.merge";

export function applyDefaults(base: Record<string, unknown>): unknown {
  return merge(base, { theme: "dark" });
}

// Known blind spot (documented, conservative false-negative): when the assignment
// target is wrapped in a TypeScript cast, the member-assign object is a
// TSAsExpression rather than a bare identifier, so the callee path does NOT
// flatten to `*.__proto__` and the matcher cannot see it. This is a real
// prototype write we miss, not a safe pattern; pinned here so a future flattening
// change that starts firing on the cast form is caught by this test flipping.
export function castProto(obj: Record<string, unknown>, evil: unknown): void {
  (obj as { __proto__: unknown }).__proto__ = evil;
}
