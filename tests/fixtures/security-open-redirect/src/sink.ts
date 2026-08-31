// Positive: a non-literal target passed to res.redirect() is an open-redirect
// candidate (CWE-601). This matcher is ungated (broad tier).
interface Res {
  redirect(target: string): void;
}

export function handle(res: Res, userTarget: string): void {
  res.redirect(userTarget);
}
