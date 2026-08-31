// Negative (literal): a fully-literal command is never captured by the extract
// layer, so it must NOT produce a command-injection candidate.
import * as child_process from "node:child_process";

export function runStatic(): void {
  child_process.exec("ls -la");
}
