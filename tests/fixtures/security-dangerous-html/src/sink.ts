// Positive: a non-literal value assigned to innerHTML is a dangerous-html candidate.
export function render(el: HTMLElement, userInput: string): void {
  el.innerHTML = userInput;
}
