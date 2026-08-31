// Negative: a fully-literal value assigned to innerHTML is never captured, so it
// must NOT produce a dangerous-html candidate.
export function renderStatic(el: HTMLElement): void {
  el.innerHTML = "<b>static</b>";
}
