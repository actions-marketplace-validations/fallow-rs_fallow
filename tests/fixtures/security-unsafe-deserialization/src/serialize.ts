// Positive: a non-literal input passed to unserialize (imported from
// node-serialize) is an unsafe-deserialization candidate (CWE-502). node-serialize
// executes embedded functions, so this is a known RCE sink.
import { unserialize } from "node-serialize";

export function revive(input: string): unknown {
  return unserialize(input);
}
