// Negative (parameterized): Drizzle / postgres.js bind `${x}` safely in a bare
// `sql`...${x}...`` tagged template, so it must NOT fire (the catalogue has no
// bare-`sql`-tag matcher). The object-form `.execute({ sql, args })` is the
// parameterized exec shape (arg_kind `object`) and is excluded by arg_kinds, so
// it must NOT fire either.
import { sql } from "drizzle-orm";

interface Db {
  execute(query: { sql: string; args: unknown[] }): Promise<unknown>;
}

export function parameterizedTemplate(userId: string): unknown {
  // Safe: the tagged template binds the interpolation as a parameter.
  return sql`SELECT * FROM users WHERE id = ${userId}`;
}

export async function parameterizedExecute(
  db: Db,
  userId: string,
): Promise<unknown> {
  // Safe: the object form carries a static sql string plus bound args.
  return db.execute({ sql: "SELECT * FROM users WHERE id = ?", args: [userId] });
}
