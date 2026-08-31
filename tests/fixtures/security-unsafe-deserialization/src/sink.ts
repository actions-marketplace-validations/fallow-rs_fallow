// Positive: a non-literal input passed to yaml.load (imported from js-yaml) is an
// unsafe-deserialization candidate (CWE-502), provenance-gated to js-yaml.
import * as yaml from "js-yaml";

export function parse(input: string): unknown {
  return yaml.load(input);
}
