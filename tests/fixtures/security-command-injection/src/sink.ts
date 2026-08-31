// Positive: a non-literal command passed to child_process.exec (imported from
// node:child_process) is a command-injection candidate (CWE-78). The binding is
// traced to the node:child_process import, mirroring the fork() provenance gate.
import * as child_process from "node:child_process";

export function run(userInput: string): void {
  child_process.exec(userInput);
}
