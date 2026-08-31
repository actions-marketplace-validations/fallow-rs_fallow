// Negative (provenance): a same-named `exec` that does NOT come from
// node:child_process must NOT fire. The matcher is binding-traced, so a local
// helper named `exec` is left alone (false-negative preferred over false-positive).
function exec(command: string): string {
  return command;
}

export function run(userInput: string): string {
  return exec(userInput);
}
