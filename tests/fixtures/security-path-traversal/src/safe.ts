// Negative (literal): a fully-literal path.join is never captured (every argument
// is a string literal), so it must NOT produce a path-traversal candidate.
import * as path from "node:path";

export function resolveStatic(): string {
  return path.join("/var/uploads", "avatar.png");
}
