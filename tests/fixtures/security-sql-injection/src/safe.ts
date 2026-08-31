// Negative (literal): a fully-literal SQL string is never captured, so it must
// NOT produce a sql-injection candidate.
interface Db {
  query(sql: string): Promise<unknown>;
}

export async function lookupAll(db: Db): Promise<unknown> {
  return db.query("SELECT * FROM users");
}
