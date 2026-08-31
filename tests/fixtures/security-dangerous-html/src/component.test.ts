// Negative (test file): an unsafe innerHTML sink inside a *.test.ts file must
// NOT produce a candidate. Test files exercise code with synthetic inputs and
// are excluded from security candidate generation (production-mode parity).
export function renderInTest(el: HTMLElement, userInput: string): void {
  el.innerHTML = userInput;
}
