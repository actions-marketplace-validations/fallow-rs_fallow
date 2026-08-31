// Negative (literal): a fully-literal redirect target is never captured, so it
// must NOT produce an open-redirect candidate.
interface Res {
  redirect(target: string): void;
}

export function handleStatic(res: Res): void {
  res.redirect("/dashboard");
}
