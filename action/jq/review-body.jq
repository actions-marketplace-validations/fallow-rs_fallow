def count(obj; key): obj | if . then .[key] // 0 else 0 end;

(count(.check; "total_issues") // 0) as $check |
(count(.dupes.stats; "clone_groups") // 0) as $dupes |
(count(.health.summary; "functions_above_threshold") // 0) as $health |
($check + $dupes + $health) as $total |
(.health.vital_signs // {}) as $vitals |
(($ENV.FILTERED_COUNT // "0") | tonumber) as $filtered |
(($ENV.INLINE_COUNT // "0") | tonumber) as $inline |

"## \ud83c\udf3f Fallow Review\n\n" +

(if $check > 0 then ":warning: **\($check)** dead code" else ":white_check_mark: No dead code" end) +
" \u00b7 " +
(if $dupes > 0 then ":warning: **\($dupes)** clone groups" else ":white_check_mark: No duplication" end) +
" \u00b7 " +
(if $health > 0 then ":warning: **\($health)** complex functions" else ":white_check_mark: Complexity OK" end) +

"\n\n" +

(if $vitals.maintainability_avg then
  "Maintainability: **\($vitals.maintainability_avg | . * 10 | round / 10)** / 100" +
  (if $vitals.avg_cyclomatic then " \u00b7 Avg complexity: \($vitals.avg_cyclomatic | . * 10 | round / 10)" else "" end) +
  "\n\n"
else "" end) +

(if $filtered > 0 then
  "**\($inline)** inline comments on your changes \u00b7 \($filtered) additional findings outside the diff\n\n"
elif $inline > 0 then
  "See inline comments for details.\n\n"
else
  "See inline comments for details.\n\n"
end) +
"<!-- fallow-review -->"
