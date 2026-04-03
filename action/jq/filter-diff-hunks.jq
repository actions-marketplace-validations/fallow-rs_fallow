# Filter review comments to only lines within PR diff hunks.
# Input: array of comment objects with {path, line, ...}
# Arg: $pr_files — GitHub API /pulls/{n}/files response
#   (array of {filename, patch, ...})
# Output: filtered array — only comments where line falls within a hunk.
#
# Fail-open: if a file has no patch (null, absent, or empty — happens for
# binary files and diffs exceeding GitHub's size limit), ALL comments for
# that file are kept rather than silently dropped.

# Parse @@ hunk headers from a unified diff patch string.
# Returns [{start, end}] for the new-file (right) side of each hunk.
def parse_hunks:
  if . == null or . == "" then []
  else
    split("\n") |
    [.[] | select(startswith("@@")) |
      capture("@@ -[0-9]+(,[0-9]+)? \\+(?<start>[0-9]+)(,(?<count>[0-9]+))? @@") |
      (.start | tonumber) as $s |
      ((.count // "1") | tonumber) as $c |
      { start: $s, end: ($s + $c - 1) }
    ]
  end;

# Build lookup: { "path": [{start, end}, ...] } for files WITH a patch.
# Files without patches are absent from this map (fail-open).
($pr_files
  | map(select(.patch != null and (.patch | length) > 0))
  | map({ key: .filename, value: (.patch | parse_hunks) })
  | from_entries
) as $hunk_map |

# Keep a comment when:
#   1. Its file is NOT in the hunk map (fail-open — no patch data), OR
#   2. Its line falls within at least one hunk range for its file.
map(select(
  .path as $p | .line as $l |
  ($hunk_map[$p] // null) as $hunks |
  if $hunks == null then true
  else ($hunks | any(.start <= $l and $l <= .end))
  end
))
