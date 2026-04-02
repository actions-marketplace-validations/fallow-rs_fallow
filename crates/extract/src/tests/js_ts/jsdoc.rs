use fallow_types::extract::ExportName;

use crate::tests::parse_ts as parse_source;

// ---- JSDoc @public tag extraction tests ----

#[test]
fn jsdoc_public_tag_on_named_export() {
    let info = parse_source("/** @public */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_on_function_export() {
    let info = parse_source("/** @public */\nexport function bar() {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_on_default_export() {
    let info = parse_source("/** @public */\nexport default function main() {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_on_class_export() {
    let info = parse_source("/** @public */\nexport class Foo {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_on_type_export() {
    let info = parse_source("/** @public */\nexport type Foo = string;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_on_interface_export() {
    let info = parse_source("/** @public */\nexport interface Bar {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_on_enum_export() {
    let info = parse_source("/** @public */\nexport enum Status { Active, Inactive }");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_multiline() {
    let info = parse_source("/**\n * Some description.\n * @public\n */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_tag_with_other_tags() {
    let info = parse_source("/** @deprecated @public */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_api_public_tag() {
    let info = parse_source("/** @api public */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn no_jsdoc_tag_not_public() {
    let info = parse_source("export const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn line_comment_not_jsdoc() {
    // Only /** */ JSDoc comments count, not // comments
    let info = parse_source("// @public\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_does_not_match_public_foo() {
    // @publicFoo should NOT match @public
    let info = parse_source("/** @publicFoo */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_does_not_match_public_underscore() {
    // @public_api should NOT match @public (underscore is an identifier char)
    let info = parse_source("/** @public_api */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_apipublic_no_space_does_not_match() {
    // @apipublic (no space) should NOT match @api public
    let info = parse_source("/** @apipublic */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_export_specifier_list() {
    let source = "const foo = 1;\nconst bar = 2;\n/** @public */\nexport { foo, bar };";
    let info = parse_source(source);
    // @public on the export statement applies to all specifiers
    assert_eq!(info.exports.len(), 2);
    assert!(info.exports[0].is_public);
    assert!(info.exports[1].is_public);
}

#[test]
fn jsdoc_public_only_applies_to_attached_export() {
    let source = "/** @public */\nexport const foo = 1;\nexport const bar = 2;";
    let info = parse_source(source);
    assert_eq!(info.exports.len(), 2);
    assert!(info.exports[0].is_public);
    assert!(!info.exports[1].is_public);
}

// ---- Additional JSDoc @public tag tests ----

#[test]
fn jsdoc_public_block_comment_not_jsdoc() {
    // /* @public */ is a block comment, not a JSDoc comment (requires /**)
    let info = parse_source("/* @public */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_anonymous_default_export() {
    let info = parse_source("/** @public */\nexport default function() {}");
    assert_eq!(info.exports.len(), 1);
    assert_eq!(info.exports[0].name, ExportName::Default);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_arrow_default_export() {
    let info = parse_source("/** @public */\nexport default () => {};");
    assert_eq!(info.exports.len(), 1);
    assert_eq!(info.exports[0].name, ExportName::Default);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_default_expression_export() {
    let info = parse_source("/** @public */\nexport default 42;");
    assert_eq!(info.exports.len(), 1);
    assert_eq!(info.exports[0].name, ExportName::Default);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_let_export() {
    let info = parse_source("/** @public */\nexport let count = 0;");
    assert_eq!(info.exports.len(), 1);
    assert_eq!(info.exports[0].name, ExportName::Named("count".to_string()));
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_with_trailing_description() {
    // @public followed by descriptive text (space-separated) should still match
    let info = parse_source("/** @public This is always exported */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_api_public_with_extra_whitespace() {
    // @api followed by multiple spaces then public
    let info = parse_source("/** @api   public */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_api_public_with_newline() {
    // @api on one line, public on the next
    let info = parse_source("/**\n * @api\n * public\n */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    // trim_start includes newlines, so "* public\n */" starts with "* public", not "public"
    // This should NOT match because there is a "* " prefix before "public"
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_api_publicfoo_does_not_match() {
    // @api publicFoo should not match (publicFoo is not standalone "public")
    let info = parse_source("/** @api publicFoo */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_multiple_exports_all_tagged() {
    let source = "/** @public */\nexport const a = 1;\n/** @public */\nexport const b = 2;";
    let info = parse_source(source);
    assert_eq!(info.exports.len(), 2);
    assert!(info.exports[0].is_public);
    assert!(info.exports[1].is_public);
}

#[test]
fn jsdoc_public_mixed_three_exports() {
    let source = "/** @public */\nexport const a = 1;\nexport const b = 2;\n/** @public */\nexport const c = 3;";
    let info = parse_source(source);
    assert_eq!(info.exports.len(), 3);
    assert!(info.exports[0].is_public);
    assert!(!info.exports[1].is_public);
    assert!(info.exports[2].is_public);
}

#[test]
fn jsdoc_public_does_not_match_numeric_suffix() {
    // @public2 should NOT match @public (digit is an ident char)
    let info = parse_source("/** @public2 */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_async_function_export() {
    let info = parse_source("/** @public */\nexport async function fetchData() {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_abstract_class_export() {
    let info = parse_source("/** @public */\nexport abstract class Base {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_star_prefix_in_multiline() {
    // Standard JSDoc with * prefix on each line
    let info = parse_source(
        "/**\n * @param x - the value\n * @returns the result\n * @public\n */\nexport const foo = 1;",
    );
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_public_on_type_alias_union() {
    let info = parse_source("/** @public */\nexport type Status = 'active' | 'inactive';");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_api_public_on_function() {
    let info = parse_source("/** @api public */\nexport function handler() {}");
    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].is_public);
}

#[test]
fn jsdoc_api_private_does_not_set_public() {
    // @api private is not @api public
    let info = parse_source("/** @api private */\nexport const foo = 1;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

#[test]
fn jsdoc_public_not_leaked_across_statements() {
    // The @public tag is on a non-export statement; the export that follows should NOT inherit it
    let source = "/** @public */\nconst internal = 1;\nexport const foo = internal;";
    let info = parse_source(source);
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}

// ── JSDoc @public tag detection ──────────────────────────────

#[test]
fn jsdoc_public_tag_marks_export_public() {
    let info = parse_source(
        r"/** @public */
export const foo = 1;",
    );
    assert_eq!(info.exports.len(), 1);
    assert!(
        info.exports[0].is_public,
        "Export with @public JSDoc tag should be marked as public"
    );
}

#[test]
fn jsdoc_api_public_tag_marks_export_public() {
    let info = parse_source(
        r"/** @api public */
export const bar = 2;",
    );
    assert_eq!(info.exports.len(), 1);
    assert!(
        info.exports[0].is_public,
        "Export with @api public tag should be marked as public"
    );
}

#[test]
fn jsdoc_no_public_tag_not_marked() {
    let info = parse_source(
        r"/** Regular comment */
export const baz = 3;",
    );
    assert_eq!(info.exports.len(), 1);
    assert!(
        !info.exports[0].is_public,
        "Export without @public tag should not be marked as public"
    );
}

#[test]
fn jsdoc_public_partial_word_not_matched() {
    let info = parse_source(
        r"/** @publicize this */
export const qux = 4;",
    );
    assert_eq!(info.exports.len(), 1);
    assert!(
        !info.exports[0].is_public,
        "@publicize should not match @public (it's followed by an ident char)"
    );
}

#[test]
fn jsdoc_public_on_function_export() {
    let info = parse_source(
        r"/** @public */
export function myFunc() { return 1; }",
    );
    let f = info
        .exports
        .iter()
        .find(|e| matches!(&e.name, ExportName::Named(n) if n == "myFunc"));
    assert!(f.is_some());
    assert!(
        f.unwrap().is_public,
        "Function export with @public should be marked as public"
    );
}

#[test]
fn jsdoc_public_on_class_export() {
    let info = parse_source(
        r"/** @public */
export class MyClass { doWork() {} }",
    );
    let c = info
        .exports
        .iter()
        .find(|e| matches!(&e.name, ExportName::Named(n) if n == "MyClass"));
    assert!(c.is_some());
    assert!(c.unwrap().is_public);
}

#[test]
fn export_without_jsdoc_not_public() {
    let info = parse_source("export const plain = 42;");
    assert_eq!(info.exports.len(), 1);
    assert!(!info.exports[0].is_public);
}
