// Positive: a non-literal SQL string built by string concatenation and passed
// to db.query() is a sql-injection candidate (CWE-89). This is an UNSAFE shape;
// the catalogue requires concat / interpolated-template arguments here.
interface Db {
  query(sql: string): Promise<unknown>;
}

export async function lookup(db: Db, userId: string): Promise<unknown> {
  return db.query("SELECT * FROM users WHERE id = " + userId);
}
