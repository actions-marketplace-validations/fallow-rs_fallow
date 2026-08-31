// Positive: a runtime-selectable algorithm passed to crypto.createHash (imported
// from node:crypto) is a weak-crypto candidate (CWE-327, retitled
// "Runtime-selectable crypto algorithm"), provenance-gated to node:crypto.
import * as crypto from "node:crypto";

export function digest(algorithm: string, data: string): string {
  return crypto.createHash(algorithm).update(data).digest("hex");
}
