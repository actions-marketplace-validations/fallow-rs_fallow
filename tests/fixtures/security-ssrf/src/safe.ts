// Negative (literal): a fully-literal URL passed to fetch() is never captured, so
// it must NOT produce an SSRF candidate.
export async function loadStatic(): Promise<Response> {
  return fetch("https://example.com/api");
}
