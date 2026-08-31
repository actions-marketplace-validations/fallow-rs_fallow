// Negative (literal): a fully-literal input is never captured, so it must NOT
// produce an unsafe-deserialization candidate.
import * as yaml from "js-yaml";

export function parseStatic(): unknown {
  return yaml.load("a: 1\nb: 2\n");
}
