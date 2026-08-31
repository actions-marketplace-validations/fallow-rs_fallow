// Positive: a non-literal script passed to vm.runInNewContext (imported from
// node:vm) is a code-injection candidate (CWE-94), provenance-gated to node:vm.
import * as vm from "node:vm";

export function runScript(userScript: string): unknown {
  return vm.runInNewContext(userScript);
}
