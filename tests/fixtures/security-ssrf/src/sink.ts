// Positive: a non-literal URL passed to fetch() is an SSRF candidate (CWE-918).
// `fetch` is a global; this matcher is ungated (broad tier).
export async function load(userUrl: string): Promise<Response> {
  return fetch(userUrl);
}
