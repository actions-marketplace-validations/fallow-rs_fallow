use std::sync::LazyLock;

use rustc_hash::FxHashSet;

use crate::template_usage::{TemplateSnippetKind, TemplateUsage, analyze_template_snippet};

static STYLE_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<style\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</style>"#)
        .expect("valid regex")
});
static SCRIPT_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<script\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</script>"#)
        .expect("valid regex")
});
static TEMPLATE_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"(?is)<template\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</template>"#,
    )
    .expect("valid regex")
});
static HTML_COMMENT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));

static SVELTE_EACH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?is)^#each\s+(?P<iterable>.+?)\s+as\s+(?P<bindings>.+?)(?:\s*\((?P<key>.+)\))?$",
    )
    .expect("valid regex")
});

static SVELTE_AWAIT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?is)^#await\s+(?P<expr>.+)$").expect("valid regex"));

static SVELTE_THEN_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^:then(?:\s+(?P<binding>.+))?$").expect("valid regex")
});

static SVELTE_CATCH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^:catch(?:\s+(?P<binding>.+))?$").expect("valid regex")
});

static SVELTE_SNIPPET_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^#snippet\s+[A-Za-z_$][\w$]*\s*\((?P<params>.*)\)\s*$")
        .expect("valid regex")
});
static VUE_FOR_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^(?P<binding>.+?)\s+(?:in|of)\s+(?P<source>.+)$").expect("valid regex")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfcKind {
    Vue,
    Svelte,
}

pub fn collect_template_usage(
    kind: SfcKind,
    source: &str,
    imported_bindings: &FxHashSet<String>,
) -> TemplateUsage {
    match kind {
        SfcKind::Vue => collect_vue_template_usage(source, imported_bindings),
        SfcKind::Svelte => collect_svelte_template_usage(source, imported_bindings),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SvelteBlockKind {
    If,
    Each,
    Await,
    Key,
    Snippet,
    Other(String),
}

#[derive(Debug, Clone)]
struct SvelteScopeFrame {
    kind: SvelteBlockKind,
    locals: Vec<String>,
}

fn collect_svelte_template_usage(
    source: &str,
    imported_bindings: &FxHashSet<String>,
) -> TemplateUsage {
    if imported_bindings.is_empty() {
        return TemplateUsage::default();
    }

    let markup = strip_svelte_non_template_content(source);
    if markup.is_empty() {
        return TemplateUsage::default();
    }

    let mut usage = TemplateUsage::default();
    let mut scopes = vec![SvelteScopeFrame {
        kind: SvelteBlockKind::Other("root".to_string()),
        locals: Vec::new(),
    }];

    let bytes = markup.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }

        let Some((tag, next_index)) = scan_svelte_brace_tag(&markup, index) else {
            break;
        };
        apply_svelte_tag(tag.trim(), imported_bindings, &mut scopes, &mut usage);
        index = next_index;
    }

    usage
}

fn collect_vue_template_usage(
    source: &str,
    imported_bindings: &FxHashSet<String>,
) -> TemplateUsage {
    if imported_bindings.is_empty() {
        return TemplateUsage::default();
    }

    let comment_ranges: Vec<(usize, usize)> = HTML_COMMENT_RE
        .find_iter(source)
        .map(|m| (m.start(), m.end()))
        .collect();

    let mut usage = TemplateUsage::default();
    for cap in TEMPLATE_BLOCK_RE.captures_iter(source) {
        let Some(template_match) = cap.get(0) else {
            continue;
        };
        if comment_ranges
            .iter()
            .any(|&(start, end)| template_match.start() >= start && template_match.start() < end)
        {
            continue;
        }
        let body = cap.name("body").map_or("", |m| m.as_str());
        merge_usage(&mut usage, scan_vue_template_body(body, imported_bindings));
    }

    usage
}

fn strip_svelte_non_template_content(source: &str) -> String {
    let mut hidden_ranges: Vec<(usize, usize)> = Vec::new();
    hidden_ranges.extend(
        HTML_COMMENT_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.extend(
        SCRIPT_BLOCK_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.extend(
        STYLE_BLOCK_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.sort_unstable_by_key(|range| range.0);

    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(hidden_ranges.len());
    for (start, end) in hidden_ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end
        {
            *last_end = (*last_end).max(end);
            continue;
        }
        merged.push((start, end));
    }

    let mut visible = String::new();
    let mut cursor = 0;
    for (start, end) in merged {
        if cursor < start {
            visible.push_str(&source[cursor..start]);
        }
        cursor = end;
    }
    if cursor < source.len() {
        visible.push_str(&source[cursor..]);
    }
    visible
}

fn scan_vue_template_body(body: &str, imported_bindings: &FxHashSet<String>) -> TemplateUsage {
    let mut usage = TemplateUsage::default();
    let mut scopes: Vec<Vec<String>> = vec![Vec::new()];
    let bytes = body.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"<!--") {
            if let Some(end) = body[index + 4..].find("-->") {
                index += 4 + end + 3;
            } else {
                break;
            }
            continue;
        }

        if bytes[index..].starts_with(b"{{") {
            let Some((expr, next_index)) = scan_vue_interpolation(body, index) else {
                break;
            };
            merge_usage(
                &mut usage,
                analyze_template_snippet(
                    expr.trim(),
                    TemplateSnippetKind::Expression,
                    imported_bindings,
                    &current_vue_locals(&scopes),
                ),
            );
            index = next_index;
            continue;
        }

        if bytes[index] == b'<' {
            let Some((tag, next_index)) = scan_html_tag(body, index) else {
                break;
            };
            apply_vue_tag(tag, imported_bindings, &mut scopes, &mut usage);
            index = next_index;
            continue;
        }

        index += 1;
    }

    usage
}

fn scan_vue_interpolation(source: &str, start: usize) -> Option<(&str, usize)> {
    debug_assert!(source.as_bytes()[start..].starts_with(b"{{"));

    let bytes = source.as_bytes();
    let mut index = start + 2;
    let mut nested_braces = 0_u32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if escape {
            escape = false;
            index += 1;
            continue;
        }

        if in_single {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'\'' {
                in_single = false;
            }
            index += 1;
            continue;
        }

        if in_double {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_double = false;
            }
            index += 1;
            continue;
        }

        if in_backtick {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'`' {
                in_backtick = false;
            }
            index += 1;
            continue;
        }

        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            line_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            block_comment = true;
            index += 2;
            continue;
        }

        match byte {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'`' => in_backtick = true,
            b'{' => nested_braces += 1,
            b'}' => {
                if nested_braces == 0 && bytes.get(index + 1) == Some(&b'}') {
                    return Some((&source[start + 2..index], index + 2));
                }
                nested_braces = nested_braces.saturating_sub(1);
            }
            _ => {}
        }

        index += 1;
    }

    None
}

fn scan_html_tag(source: &str, start: usize) -> Option<(&str, usize)> {
    debug_assert_eq!(source.as_bytes().get(start), Some(&b'<'));

    let bytes = source.as_bytes();
    let mut index = start + 1;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if escape {
            escape = false;
            index += 1;
            continue;
        }

        if in_single {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'\'' {
                in_single = false;
            }
            index += 1;
            continue;
        }

        if in_double {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_double = false;
            }
            index += 1;
            continue;
        }

        match byte {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'>' => return Some((&source[start..=index], index + 1)),
            _ => index += 1,
        }
    }

    None
}

fn apply_vue_tag(
    tag: &str,
    imported_bindings: &FxHashSet<String>,
    scopes: &mut Vec<Vec<String>>,
    usage: &mut TemplateUsage,
) {
    let trimmed = tag.trim();
    if trimmed.starts_with("</") {
        if scopes.len() > 1 {
            scopes.pop();
        }
        return;
    }

    if trimmed.starts_with("<!") || trimmed.starts_with("<?") {
        return;
    }

    let parsed = parse_vue_tag(trimmed);
    let current = current_vue_locals(scopes);

    let mut element_locals = Vec::new();
    if let Some(value) = parsed
        .attrs
        .iter()
        .find(|attr| attr.name == "v-for")
        .and_then(|attr| attr.value.as_deref())
        && let Some(captures) = VUE_FOR_RE.captures(value)
    {
        let binding = captures.name("binding").map_or("", |m| m.as_str()).trim();
        let source_expr = captures.name("source").map_or("", |m| m.as_str()).trim();
        merge_usage(
            usage,
            analyze_template_snippet(
                source_expr,
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current,
            ),
        );
        element_locals.extend(extract_pattern_binding_names(binding));
    }

    if let Some(value) = parsed
        .attrs
        .iter()
        .find(|attr| {
            attr.name == "slot-scope"
                || attr.name.starts_with("v-slot")
                || attr.name.starts_with('#')
        })
        .and_then(|attr| attr.value.as_deref())
    {
        element_locals.extend(extract_pattern_binding_names(value));
    }

    let mut attr_locals = current;
    attr_locals.extend(element_locals.iter().cloned());
    for attr in &parsed.attrs {
        if let Some(expr) = attr.value.as_deref() {
            if attr.name == "v-for"
                || attr.name == "slot-scope"
                || attr.name.starts_with("v-slot")
                || attr.name.starts_with('#')
            {
                continue;
            }

            if is_vue_statement_attr(&attr.name) {
                merge_usage(
                    usage,
                    analyze_template_snippet(
                        expr,
                        TemplateSnippetKind::Statement,
                        imported_bindings,
                        &attr_locals,
                    ),
                );
            } else if is_vue_expression_attr(&attr.name) {
                merge_usage(
                    usage,
                    analyze_template_snippet(
                        expr,
                        TemplateSnippetKind::Expression,
                        imported_bindings,
                        &attr_locals,
                    ),
                );
            }
        }
    }

    if !parsed.self_closing {
        scopes.push(element_locals);
    }
}

fn current_vue_locals(scopes: &[Vec<String>]) -> Vec<String> {
    scopes
        .iter()
        .flat_map(|locals| locals.iter().cloned())
        .collect()
}

#[derive(Debug)]
struct VueTag {
    attrs: Vec<VueAttr>,
    self_closing: bool,
}

#[derive(Debug)]
struct VueAttr {
    name: String,
    value: Option<String>,
}

fn parse_vue_tag(tag: &str) -> VueTag {
    let inner = tag.trim_start_matches('<').trim_end_matches('>').trim();
    let self_closing = inner.ends_with('/');
    let inner = inner.trim_end_matches('/').trim_end();

    let mut attrs = Vec::new();
    let mut index = inner
        .char_indices()
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx))
        .unwrap_or(inner.len());

    while index < inner.len() {
        let remaining = &inner[index..];
        let trimmed = remaining.trim_start();
        index += remaining.len() - trimmed.len();
        if index >= inner.len() {
            break;
        }

        let name_end = inner[index..]
            .char_indices()
            .find_map(|(offset, ch)| (ch.is_whitespace() || ch == '=').then_some(index + offset))
            .unwrap_or(inner.len());
        let name = inner[index..name_end].trim();
        index = name_end;
        let remaining = &inner[index..];
        let trimmed = remaining.trim_start();
        index += remaining.len() - trimmed.len();

        let mut value = None;
        if inner.as_bytes().get(index) == Some(&b'=') {
            index += 1;
            let remaining = &inner[index..];
            let trimmed = remaining.trim_start();
            index += remaining.len() - trimmed.len();
            if let Some(quote) = inner.as_bytes().get(index).copied() {
                if quote == b'\'' || quote == b'"' {
                    let quote = quote as char;
                    index += 1;
                    let value_start = index;
                    while index < inner.len() && inner.as_bytes()[index] as char != quote {
                        index += 1;
                    }
                    value = Some(inner[value_start..index].to_string());
                    if index < inner.len() {
                        index += 1;
                    }
                } else {
                    let value_end = inner[index..]
                        .char_indices()
                        .find_map(|(offset, ch)| ch.is_whitespace().then_some(index + offset))
                        .unwrap_or(inner.len());
                    value = Some(inner[index..value_end].to_string());
                    index = value_end;
                }
            }
        }

        if !name.is_empty() {
            attrs.push(VueAttr {
                name: name.to_string(),
                value,
            });
        }
    }

    VueTag {
        attrs,
        self_closing,
    }
}

fn is_vue_statement_attr(name: &str) -> bool {
    name.starts_with('@') || name.starts_with("v-on:")
}

fn is_vue_expression_attr(name: &str) -> bool {
    name.starts_with(':')
        || name.starts_with("v-bind:")
        || matches!(
            name,
            "v-if" | "v-else-if" | "v-show" | "v-html" | "v-text" | "v-memo" | "v-model"
        )
        || name.starts_with("v-model:")
}

fn scan_svelte_brace_tag(source: &str, start: usize) -> Option<(&str, usize)> {
    debug_assert_eq!(source.as_bytes().get(start), Some(&b'{'));

    let bytes = source.as_bytes();
    let mut index = start + 1;
    let mut nested_braces = 0_u32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if escape {
            escape = false;
            index += 1;
            continue;
        }

        if in_single {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'\'' {
                in_single = false;
            }
            index += 1;
            continue;
        }

        if in_double {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_double = false;
            }
            index += 1;
            continue;
        }

        if in_backtick {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'`' {
                in_backtick = false;
            }
            index += 1;
            continue;
        }

        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            line_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            block_comment = true;
            index += 2;
            continue;
        }

        match byte {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'`' => in_backtick = true,
            b'{' => nested_braces += 1,
            b'}' => {
                if nested_braces == 0 {
                    return Some((&source[start + 1..index], index + 1));
                }
                nested_braces -= 1;
            }
            _ => {}
        }

        index += 1;
    }

    None
}

fn apply_svelte_tag(
    tag: &str,
    imported_bindings: &FxHashSet<String>,
    scopes: &mut Vec<SvelteScopeFrame>,
    usage: &mut TemplateUsage,
) {
    if tag.is_empty() {
        return;
    }

    if let Some(rest) = tag.strip_prefix('/') {
        pop_svelte_scope(scopes, rest.trim());
        return;
    }

    if let Some(expr) = tag.strip_prefix("#if") {
        merge_usage(
            usage,
            analyze_template_snippet(
                expr.trim(),
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::If,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_EACH_RE.captures(tag) {
        let iterable = captures.name("iterable").map_or("", |m| m.as_str()).trim();
        let bindings = captures.name("bindings").map_or("", |m| m.as_str()).trim();
        let each_locals = extract_pattern_binding_names(bindings);
        let current = current_locals(scopes);
        merge_usage(
            usage,
            analyze_template_snippet(
                iterable,
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current,
            ),
        );
        if let Some(key) = captures.name("key").map(|m| m.as_str().trim())
            && !key.is_empty()
        {
            let mut key_locals = current;
            key_locals.extend(each_locals.iter().cloned());
            merge_usage(
                usage,
                analyze_template_snippet(
                    key,
                    TemplateSnippetKind::Expression,
                    imported_bindings,
                    &key_locals,
                ),
            );
        }
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Each,
            locals: each_locals,
        });
        return;
    }

    if let Some(captures) = SVELTE_AWAIT_RE.captures(tag) {
        let expr = captures.name("expr").map_or("", |m| m.as_str()).trim();
        merge_usage(
            usage,
            analyze_template_snippet(
                expr,
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Await,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_THEN_RE.captures(tag) {
        if let Some(frame) = scopes
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.kind, SvelteBlockKind::Await))
        {
            frame.locals = captures
                .name("binding")
                .map(|m| extract_pattern_binding_names(m.as_str()))
                .unwrap_or_default();
        }
        return;
    }

    if let Some(captures) = SVELTE_CATCH_RE.captures(tag) {
        if let Some(frame) = scopes
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.kind, SvelteBlockKind::Await))
        {
            frame.locals = captures
                .name("binding")
                .map(|m| extract_pattern_binding_names(m.as_str()))
                .unwrap_or_default();
        }
        return;
    }

    if let Some(expr) = tag.strip_prefix("#key") {
        merge_usage(
            usage,
            analyze_template_snippet(
                expr.trim(),
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Key,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_SNIPPET_RE.captures(tag) {
        let params = captures.name("params").map_or("", |m| m.as_str());
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Snippet,
            locals: extract_pattern_binding_names(params),
        });
        return;
    }

    if let Some(expr) = tag.strip_prefix("@html") {
        merge_usage(
            usage,
            analyze_template_snippet(
                expr.trim(),
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        return;
    }

    if let Some(expr) = tag.strip_prefix("@render") {
        merge_usage(
            usage,
            analyze_template_snippet(
                expr.trim(),
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        return;
    }

    if let Some(stmt) = tag.strip_prefix("@const") {
        let locals = current_locals(scopes);
        merge_usage(
            usage,
            analyze_template_snippet(
                stmt.trim(),
                TemplateSnippetKind::Statement,
                imported_bindings,
                &locals,
            ),
        );
        if let Some(lhs) = stmt.split_once('=').map(|(lhs, _)| lhs.trim()) {
            let new_bindings = extract_pattern_binding_names(lhs);
            if let Some(frame) = scopes.last_mut() {
                frame.locals.extend(new_bindings);
            }
        }
        return;
    }

    if let Some(expr) = tag.strip_prefix("@debug") {
        merge_usage(
            usage,
            analyze_template_snippet(
                expr.trim(),
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        return;
    }

    if let Some(expr) = tag.strip_prefix(":else if") {
        merge_usage(
            usage,
            analyze_template_snippet(
                expr.trim(),
                TemplateSnippetKind::Expression,
                imported_bindings,
                &current_locals(scopes),
            ),
        );
        return;
    }

    if tag.starts_with(":else") {
        return;
    }

    merge_usage(
        usage,
        analyze_template_snippet(
            tag,
            TemplateSnippetKind::Expression,
            imported_bindings,
            &current_locals(scopes),
        ),
    );
}

fn pop_svelte_scope(scopes: &mut Vec<SvelteScopeFrame>, closing: &str) {
    let kind = match closing {
        "if" => Some(SvelteBlockKind::If),
        "each" => Some(SvelteBlockKind::Each),
        "await" => Some(SvelteBlockKind::Await),
        "key" => Some(SvelteBlockKind::Key),
        "snippet" => Some(SvelteBlockKind::Snippet),
        _ => None,
    };

    let Some(kind) = kind else {
        return;
    };

    if let Some(index) = scopes.iter().rposition(|frame| frame.kind == kind)
        && index > 0
    {
        scopes.truncate(index);
    }
}

fn current_locals(scopes: &[SvelteScopeFrame]) -> Vec<String> {
    scopes
        .iter()
        .flat_map(|frame| frame.locals.iter().cloned())
        .collect()
}

fn merge_usage(into: &mut TemplateUsage, other: TemplateUsage) {
    into.used_bindings.extend(other.used_bindings);
    for access in other.member_accesses {
        let already_present = into
            .member_accesses
            .iter()
            .any(|existing| existing.object == access.object && existing.member == access.member);
        if !already_present {
            into.member_accesses.push(access);
        }
    }
    for whole in other.whole_object_uses {
        if !into
            .whole_object_uses
            .iter()
            .any(|existing| existing == &whole)
        {
            into.whole_object_uses.push(whole);
        }
    }
}

fn extract_pattern_binding_names(pattern: &str) -> Vec<String> {
    let pattern = trim_outer_parens(pattern.trim());
    if pattern.is_empty() {
        return Vec::new();
    }

    if let Some(inner) = strip_wrapping(pattern, '{', '}') {
        return split_top_level(inner, ',')
            .into_iter()
            .flat_map(|part| {
                let part = part.trim();
                if part.is_empty() || part == "..." {
                    return Vec::new();
                }
                let part = part.strip_prefix("...").unwrap_or(part).trim();
                if let Some((_, rhs)) = split_top_level_once(part, ':') {
                    return extract_pattern_binding_names(rhs);
                }
                if let Some((lhs, _)) = split_top_level_once(part, '=') {
                    return extract_pattern_binding_names(lhs);
                }
                extract_pattern_binding_names(part)
            })
            .collect();
    }

    if let Some(inner) = strip_wrapping(pattern, '[', ']') {
        return split_top_level(inner, ',')
            .into_iter()
            .flat_map(|part| extract_pattern_binding_names(part.trim()))
            .collect();
    }

    if pattern.contains(',') {
        return split_top_level(pattern, ',')
            .into_iter()
            .flat_map(|part| extract_pattern_binding_names(part.trim()))
            .collect();
    }

    if let Some((lhs, _)) = split_top_level_once(pattern, '=') {
        return extract_pattern_binding_names(lhs);
    }

    valid_identifier(pattern)
        .map(|ident| vec![ident.to_string()])
        .unwrap_or_default()
}

fn split_top_level(source: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for (idx, ch) in source.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            _ if in_single || in_double || in_backtick => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                parts.push(&source[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&source[start..]);
    parts
}

fn split_top_level_once(source: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut depth = 0_i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for (idx, ch) in source.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            _ if in_single || in_double || in_backtick => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                let rhs = &source[idx + ch.len_utf8()..];
                return Some((&source[..idx], rhs));
            }
            _ => {}
        }
    }
    None
}

fn strip_wrapping(source: &str, open: char, close: char) -> Option<&str> {
    source
        .strip_prefix(open)
        .and_then(|inner| inner.strip_suffix(close))
}

fn trim_outer_parens(source: &str) -> &str {
    source
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(source)
}

fn valid_identifier(source: &str) -> Option<&str> {
    let mut chars = source.chars();
    let first = chars.next()?;
    if !matches!(first, 'A'..='Z' | 'a'..='z' | '_' | '$') {
        return None;
    }
    chars
        .all(|ch| matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '$'))
        .then_some(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imported(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn svelte_plain_expression_marks_binding_used() {
        let usage = collect_template_usage(
            SfcKind::Svelte,
            "<script>import { formatDate } from './utils';</script><p>{formatDate(value)}</p>",
            &imported(&["formatDate"]),
        );

        assert!(usage.used_bindings.contains("formatDate"));
    }

    #[test]
    fn svelte_each_alias_shadows_import_name() {
        let usage = collect_template_usage(
            SfcKind::Svelte,
            "<script>import { item } from './utils';</script>{#each items as item}<p>{item}</p>{/each}",
            &imported(&["item"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn svelte_namespace_member_accesses_are_retained() {
        let usage = collect_template_usage(
            SfcKind::Svelte,
            "<script>import * as utils from './utils';</script><p>{utils.formatDate(value)}</p>",
            &imported(&["utils"]),
        );

        assert!(usage.used_bindings.contains("utils"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "utils");
        assert_eq!(usage.member_accesses[0].member, "formatDate");
    }

    #[test]
    fn svelte_styles_are_ignored() {
        let usage = collect_template_usage(
            SfcKind::Svelte,
            "<style>.button { color: red; }</style><script>import { button } from './utils';</script>",
            &imported(&["button"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn vue_mustache_marks_binding_used() {
        let usage = collect_template_usage(
            SfcKind::Vue,
            "<script setup>import { formatDate } from './utils';</script><template><p>{{ formatDate(value) }}</p></template>",
            &imported(&["formatDate"]),
        );

        assert!(usage.used_bindings.contains("formatDate"));
    }

    #[test]
    fn vue_v_for_alias_shadows_import_name() {
        let usage = collect_template_usage(
            SfcKind::Vue,
            "<script setup>import { item } from './utils';</script><template><li v-for=\"item in items\">{{ item }}</li></template>",
            &imported(&["item"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn vue_namespace_member_accesses_are_retained() {
        let usage = collect_template_usage(
            SfcKind::Vue,
            "<script setup>import * as utils from './utils';</script><template><p>{{ utils.formatDate(value) }}</p></template>",
            &imported(&["utils"]),
        );

        assert!(usage.used_bindings.contains("utils"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "utils");
        assert_eq!(usage.member_accesses[0].member, "formatDate");
    }

    #[test]
    fn vue_event_handlers_are_treated_as_statements() {
        let usage = collect_template_usage(
            SfcKind::Vue,
            "<script setup>import { increment } from './utils';</script><template><button @click=\"count += increment(step)\">Add</button></template>",
            &imported(&["increment"]),
        );

        assert!(usage.used_bindings.contains("increment"));
    }
}
