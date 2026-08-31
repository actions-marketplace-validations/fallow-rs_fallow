// Negative (build config): an unsafe innerHTML sink inside a tooling config file
// must NOT produce a candidate. Build configs run at build time and are excluded
// from security candidate generation (production-mode parity).
export function configurePreview(el: HTMLElement, userInput: string): void {
  el.innerHTML = userInput;
}
