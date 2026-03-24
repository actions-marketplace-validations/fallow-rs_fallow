/// Find a suitable insert position at module scope above the first instance.
///
/// Walks backwards from `first_start_0based` looking for an empty line or a
/// line that starts at column 0 (module scope), to avoid inserting inside a
/// function body.
pub(super) fn find_insert_line(first_start_0based: u32, file_lines: &[&str]) -> u32 {
    let mut line = first_start_0based;
    while line > 0 {
        line -= 1;
        let content = file_lines.get(line as usize).copied().unwrap_or("");
        // An empty line or a line starting at column 0 (module scope) is a good insert point
        if content.is_empty() || (!content.starts_with(' ') && !content.starts_with('\t')) {
            break;
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_insert_line_stops_at_empty_line() {
        let lines = vec!["function a() {", "  return 1;", "}", "", "  const x = 1;"];
        // Searching backwards from line 4, should stop at empty line 3
        assert_eq!(find_insert_line(4, &lines), 3);
    }

    #[test]
    fn find_insert_line_stops_at_module_scope_line() {
        let lines = vec!["const a = 1;", "  indented code", "  more indented"];
        // Searching backwards from line 2, hits "  indented code" (starts with space),
        // then hits "const a = 1;" (no leading space/tab) at line 0
        assert_eq!(find_insert_line(2, &lines), 0);
    }

    #[test]
    fn find_insert_line_returns_0_when_all_indented() {
        let lines = vec!["  a", "  b", "  c"];
        // Searching backwards from line 2: line 1 is indented, line 0 is indented
        // Loop goes: line=1 (indented, continue), line=0 (indented, continue),
        // loop ends (line > 0 is false). Returns 0.
        assert_eq!(find_insert_line(2, &lines), 0);
    }

    #[test]
    fn find_insert_line_at_line_0_returns_0() {
        let lines = vec!["  something"];
        // first_start_0based is 0, while loop condition is line > 0, so loop never runs
        assert_eq!(find_insert_line(0, &lines), 0);
    }

    #[test]
    fn find_insert_line_stops_at_line_starting_with_text() {
        let lines = vec![
            "import { x } from 'y';",
            "export function foo() {",
            "  return x;",
            "}",
            "  // indented comment",
            "  const z = 1;",
        ];
        // From line 5, walk back: line 4 (indented), line 3 ("}" at col 0) => stop
        assert_eq!(find_insert_line(5, &lines), 3);
    }

    #[test]
    fn find_insert_line_empty_source_returns_0() {
        let lines: Vec<&str> = vec![];
        // first_start_0based is 0, loop never runs
        assert_eq!(find_insert_line(0, &lines), 0);
    }

    #[test]
    fn find_insert_line_walks_past_nested_function() {
        let lines = vec![
            "const top = 1;",        // 0 - module scope
            "",                       // 1 - empty line
            "function outer() {",     // 2 - module scope
            "  function inner() {",   // 3 - indented
            "    return 42;",         // 4 - indented
            "  }",                    // 5 - indented
            "  const x = inner();",   // 6 - indented (target)
        ];
        // From line 6, walk back: 5 (indented), 4 (indented), 3 (indented),
        // 2 ("function outer" at col 0) => stop
        assert_eq!(find_insert_line(6, &lines), 2);
    }

    #[test]
    fn find_insert_line_multiple_nesting_levels() {
        let lines = vec![
            "class Foo {",                 // 0 - module scope
            "  method() {",                // 1 - indented
            "    if (true) {",             // 2 - indented
            "      for (;;) {",            // 3 - indented
            "        doSomething();",       // 4 - indented
            "      }",                     // 5 - indented
            "    }",                       // 6 - indented
            "  }",                         // 7 - indented
            "}",                           // 8 - module scope
        ];
        // From line 4, walk back: 3 (indented), 2 (indented), 1 (indented),
        // 0 ("class Foo" at col 0) => stop
        assert_eq!(find_insert_line(4, &lines), 0);
    }

    #[test]
    fn find_insert_line_at_end_of_file() {
        let lines = vec![
            "import { a } from 'a';",  // 0 - module scope
            "",                         // 1 - empty
            "export function run() {",  // 2 - module scope
            "  return a();",            // 3 - indented
            "}",                        // 4 - module scope
            "",                         // 5 - empty
            "  orphanedCode();",        // 6 - indented (last line)
        ];
        // From line 6, walk back: 5 (empty) => stop
        assert_eq!(find_insert_line(6, &lines), 5);
    }
}
