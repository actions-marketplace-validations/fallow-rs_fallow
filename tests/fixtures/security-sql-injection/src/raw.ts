// Positive: Drizzle's `sql.raw(...)` bypasses parameterization, so a non-literal
// argument is a sql-injection candidate (CWE-89). This is the documented
// injection escape hatch.
import { sql } from "drizzle-orm";

export function rawFragment(column: string): unknown {
  return sql.raw(column);
}
