/// Strip common leading whitespace from a fragment and re-indent with 2 spaces.
pub(super) fn dedent_fragment(fragment: &str) -> String {
    let common_indent = fragment
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    fragment
        .lines()
        .map(|line| {
            let stripped = if line.len() > common_indent {
                &line[common_indent..]
            } else {
                line.trim_start()
            };
            if stripped.is_empty() {
                String::new()
            } else {
                format!("  {stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedent_removes_common_indent_and_adds_two_spaces() {
        let fragment = "    const x = 1;\n    const y = 2;";
        let result = dedent_fragment(fragment);
        assert_eq!(result, "  const x = 1;\n  const y = 2;");
    }

    #[test]
    fn dedent_handles_mixed_indent_levels() {
        let fragment = "    if (true) {\n        return 1;\n    }";
        let result = dedent_fragment(fragment);
        // Common indent is 4, so 4 removed from each line
        assert_eq!(result, "  if (true) {\n      return 1;\n  }");
    }

    #[test]
    fn dedent_skips_empty_lines_for_common_indent_calculation() {
        let fragment = "    const x = 1;\n\n    const y = 2;";
        let result = dedent_fragment(fragment);
        // Empty line should become empty (no indent added), others get 2-space indent
        assert_eq!(result, "  const x = 1;\n\n  const y = 2;");
    }

    #[test]
    fn dedent_handles_no_indentation() {
        let fragment = "const x = 1;\nconst y = 2;";
        let result = dedent_fragment(fragment);
        // Common indent is 0, so each line gets 2 spaces prepended
        assert_eq!(result, "  const x = 1;\n  const y = 2;");
    }

    #[test]
    fn dedent_handles_single_line() {
        let fragment = "  return 42;";
        let result = dedent_fragment(fragment);
        assert_eq!(result, "  return 42;");
    }

    #[test]
    fn dedent_handles_all_empty_lines() {
        let fragment = "\n\n";
        let result = dedent_fragment(fragment);
        // Rust's .lines() yields ["", ""] for "\n\n" (no trailing empty element).
        // Both are empty so they stay empty. Joined with "\n" => "\n".
        assert_eq!(result, "\n");
    }

    #[test]
    fn dedent_handles_empty_fragment() {
        let result = dedent_fragment("");
        assert_eq!(result, "");
    }

    #[test]
    fn dedent_single_line_no_indent() {
        let result = dedent_fragment("return 42;");
        // Common indent is 0, so it gets 2-space prefix
        assert_eq!(result, "  return 42;");
    }

    #[test]
    fn dedent_mixed_tabs_and_spaces() {
        let fragment = "\t  const x = 1;\n\t  const y = 2;";
        let result = dedent_fragment(fragment);
        // Common indent is 3 (tab + 2 spaces), stripped then 2-space prefix
        assert_eq!(result, "  const x = 1;\n  const y = 2;");
    }

    #[test]
    fn dedent_all_lines_same_indent_stripped_to_zero() {
        let fragment = "      a();\n      b();\n      c();";
        let result = dedent_fragment(fragment);
        // Common indent is 6, all stripped to 0 then re-indented with 2 spaces
        assert_eq!(result, "  a();\n  b();\n  c();");
    }

    #[test]
    fn dedent_whitespace_only_lines_treated_as_blank() {
        // Middle line has only spaces — should be treated like a blank line
        let fragment = "    const x = 1;\n    \n    const y = 2;";
        let result = dedent_fragment(fragment);
        // The whitespace-only line (4 spaces) is shorter than common_indent after
        // trim_start yields empty, so it becomes empty. Other lines get 2-space prefix.
        assert_eq!(result, "  const x = 1;\n\n  const y = 2;");
    }
}
