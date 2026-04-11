use rustc_hash::{FxHashMap, FxHashSet};

use crate::discover::FileId;
use crate::extract::{ANGULAR_TPL_SENTINEL, MemberKind};
use crate::graph::ModuleGraph;
use crate::resolve::{ResolveResult, ResolvedModule};
use crate::results::UnusedMember;
use crate::suppress::{self, IssueKind, Suppression};

use super::predicates::{is_angular_lifecycle_method, is_react_lifecycle_method};
use super::{LineOffsetsMap, byte_offset_to_line_col};

/// Find unused enum and class members in exported symbols.
///
/// Collects all `Identifier.member` static member accesses from all modules,
/// maps them to their imported names, and filters out members that are accessed.
///
/// `user_class_member_allowlist` extends the built-in Angular/React lifecycle
/// allowlist with framework-invoked method names contributed by plugins and
/// top-level config (see `FallowConfig::used_class_members` and
/// `Plugin::used_class_members`). Members whose name is in this set are never
/// flagged as unused-class-members — used for third-party interface patterns
/// where a library calls consumer methods reflectively (ag-Grid's `agInit`,
/// Web Components' `connectedCallback`, etc.).
#[expect(
    clippy::too_many_lines,
    reason = "member tracking requires many graph traversal steps; split candidate for sig-audit-loop"
)]
pub fn find_unused_members(
    graph: &ModuleGraph,
    resolved_modules: &[ResolvedModule],
    suppressions_by_file: &FxHashMap<FileId, &[Suppression]>,
    line_offsets_by_file: &LineOffsetsMap<'_>,
    user_class_member_allowlist: &FxHashSet<&str>,
) -> (Vec<UnusedMember>, Vec<UnusedMember>) {
    let mut unused_enum_members = Vec::new();
    let mut unused_class_members = Vec::new();

    // Map export_name -> set of member_names that are accessed across all modules.
    // We map local import names back to the original imported names.
    let mut accessed_members: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    // Also build a per-file set of `this.member` accesses. These indicate internal usage
    // within a class body — class members accessed via `this.foo` are used internally
    // even if no external code accesses them via `ClassName.foo`.
    let mut self_accessed_members: FxHashMap<crate::discover::FileId, FxHashSet<String>> =
        FxHashMap::default();

    // Build a set of export names that are used as whole objects (Object.values, for..in, etc.).
    // All members of these exports should be considered used.
    let mut whole_object_used_exports: FxHashSet<String> = FxHashSet::default();

    for resolved in resolved_modules {
        // Build a map from local name -> imported name for this module's imports
        let local_to_imported: FxHashMap<&str, &str> = resolved
            .resolved_imports
            .iter()
            .filter_map(|imp| match &imp.info.imported_name {
                crate::extract::ImportedName::Named(name) => {
                    Some((imp.info.local_name.as_str(), name.as_str()))
                }
                crate::extract::ImportedName::Default => {
                    Some((imp.info.local_name.as_str(), "default"))
                }
                _ => None,
            })
            .collect();

        for access in &resolved.member_accesses {
            // Track `this.member` accesses per-file for internal class usage
            if access.object == "this" {
                self_accessed_members
                    .entry(resolved.file_id)
                    .or_default()
                    .insert(access.member.clone());
                continue;
            }
            // If the object is a local name for an import, map it to the original export name
            let export_name = local_to_imported
                .get(access.object.as_str())
                .copied()
                .unwrap_or(access.object.as_str());
            accessed_members
                .entry(export_name.to_string())
                .or_default()
                .insert(access.member.clone());
        }

        // Map whole-object uses from local names to imported names
        for local_name in &resolved.whole_object_uses {
            let export_name = local_to_imported
                .get(local_name.as_str())
                .copied()
                .unwrap_or(local_name.as_str());
            whole_object_used_exports.insert(export_name.to_string());
        }
    }

    // ── Inheritance propagation ────────────────────────────────────────────
    //
    // Build an inheritance map from `extends` clauses, then propagate member
    // accesses through the hierarchy. This prevents false positives where:
    // - A parent class method accesses `this.member` (credits child overrides)
    // - A child class override is flagged unused when the parent method is used
    //
    // Maps are scoped by (parent_name, parent_file_id) to avoid collisions
    // when two files each export a class with the same name.
    //
    // parent_to_children: ("BaseShape", FileId) → ["Circle", "Rectangle"]
    let mut parent_to_children: FxHashMap<(String, FileId), Vec<String>> = FxHashMap::default();

    for resolved in resolved_modules {
        // Build local→imported map for resolving super_class names
        let local_to_imported: FxHashMap<&str, (&str, FileId)> = resolved
            .resolved_imports
            .iter()
            .filter_map(|imp| {
                let imported_name = match &imp.info.imported_name {
                    crate::extract::ImportedName::Named(name) => name.as_str(),
                    crate::extract::ImportedName::Default => "default",
                    _ => return None,
                };
                if let ResolveResult::InternalModule(file_id) = &imp.target {
                    Some((imp.info.local_name.as_str(), (imported_name, *file_id)))
                } else {
                    None
                }
            })
            .collect();

        for export in &resolved.exports {
            if let Some(super_local) = &export.super_class {
                let child_name = export.name.to_string();
                if let Some(&(parent_name, parent_file_id)) =
                    local_to_imported.get(super_local.as_str())
                {
                    parent_to_children
                        .entry((parent_name.to_string(), parent_file_id))
                        .or_default()
                        .push(child_name);
                } else {
                    // Parent class defined in the same file (no import needed)
                    parent_to_children
                        .entry((super_local.clone(), resolved.file_id))
                        .or_default()
                        .push(child_name);
                }
            }
        }
    }

    // Propagate `this.member` accesses from parent files to child files.
    // When BaseShape.describe() calls `this.getArea()`, that should credit
    // Circle.getArea() and Rectangle.getArea() as used.
    if !parent_to_children.is_empty() {
        // Build export-name → file_id index for O(1) child lookup
        let export_name_to_file: FxHashMap<String, FileId> = graph
            .modules
            .iter()
            .flat_map(|m| {
                m.exports
                    .iter()
                    .map(move |e| (e.name.to_string(), m.file_id))
            })
            .collect();

        // Collect propagations first to avoid borrow conflicts
        let mut propagations: Vec<(FileId, Vec<String>)> = Vec::new();

        for ((parent_name, parent_fid), children) in &parent_to_children {
            // Propagate parent's this.* accesses to child files
            if let Some(parent_self_accesses) = self_accessed_members.get(parent_fid) {
                let accesses: Vec<String> = parent_self_accesses.iter().cloned().collect();
                for child_name in children {
                    if let Some(&child_fid) = export_name_to_file.get(child_name.as_str()) {
                        propagations.push((child_fid, accesses.clone()));
                    }
                }
            }

            // Also propagate accessed_members bidirectionally:
            // If parent's member is externally accessed, credit all children
            // If child's member is externally accessed, credit the parent
            let parent_accesses: Option<FxHashSet<String>> =
                accessed_members.get(parent_name.as_str()).cloned();
            let mut child_accesses_to_propagate: FxHashSet<String> = FxHashSet::default();

            for child_name in children {
                if let Some(child_accesses) = accessed_members.get(child_name.as_str()) {
                    child_accesses_to_propagate.extend(child_accesses.iter().cloned());
                }
            }

            // Parent → children
            if let Some(ref parent_acc) = parent_accesses {
                for child_name in children {
                    accessed_members
                        .entry(child_name.clone())
                        .or_default()
                        .extend(parent_acc.iter().cloned());
                }
            }

            // Children → parent
            if !child_accesses_to_propagate.is_empty() {
                accessed_members
                    .entry(parent_name.clone())
                    .or_default()
                    .extend(child_accesses_to_propagate);
            }
        }

        // Apply self_accessed_members propagations
        for (file_id, members) in propagations {
            let entry = self_accessed_members.entry(file_id).or_default();
            for member in members {
                entry.insert(member);
            }
        }
    }

    // Bridge Angular template member refs to their owning components.
    //
    // Sentinel member accesses come from two sources:
    // 1. External templates: HTML files scanned for Angular syntax, with sentinel
    //    accesses stored on the HTML file's ModuleInfo. Bridged to the component
    //    via the SideEffect import edge from @Component({ templateUrl }).
    // 2. Inline templates/host/inputs/outputs: sentinel accesses stored directly
    //    on the component's own ModuleInfo (same file as the class).
    let angular_tpl_refs: FxHashMap<FileId, Vec<&str>> = resolved_modules
        .iter()
        .filter_map(|m| {
            let refs: Vec<&str> = m
                .member_accesses
                .iter()
                .filter(|a| a.object == ANGULAR_TPL_SENTINEL)
                .map(|a| a.member.as_str())
                .collect();
            if refs.is_empty() {
                None
            } else {
                Some((m.file_id, refs))
            }
        })
        .collect();

    if !angular_tpl_refs.is_empty() {
        for resolved in resolved_modules {
            // Case 1: sentinel accesses on the same file (inline template, host, inputs/outputs)
            if let Some(refs) = angular_tpl_refs.get(&resolved.file_id) {
                let entry = self_accessed_members.entry(resolved.file_id).or_default();
                for &ref_name in refs {
                    entry.insert(ref_name.to_string());
                }
            }
            // Case 2: sentinel accesses on an imported file (external templateUrl)
            for import in &resolved.resolved_imports {
                if let ResolveResult::InternalModule(target_id) = &import.target
                    && let Some(refs) = angular_tpl_refs.get(target_id)
                {
                    let entry = self_accessed_members.entry(resolved.file_id).or_default();
                    for &ref_name in refs {
                        entry.insert(ref_name.to_string());
                    }
                }
            }
        }
    }

    for module in &graph.modules {
        if !module.is_reachable() || module.is_entry_point() {
            continue;
        }

        for export in &module.exports {
            if export.members.is_empty() {
                continue;
            }

            // If the export itself is unused, skip member analysis (whole export is dead)
            if export.references.is_empty() && !graph.has_namespace_import(module.file_id) {
                continue;
            }

            let export_name = export.name.to_string();

            // If this export is used as a whole object (Object.values, for..in, etc.),
            // all members are considered used — skip individual member analysis.
            if whole_object_used_exports.contains(&export_name) {
                continue;
            }

            // Get `this.member` accesses from this file (internal class usage)
            let file_self_accesses = self_accessed_members.get(&module.file_id);

            for member in &export.members {
                // Skip namespace members for now — individual namespace member
                // unused detection is a future enhancement. The namespace as a
                // whole is already tracked via unused export detection.
                if matches!(member.kind, MemberKind::NamespaceMember) {
                    continue;
                }

                // Check if this member is accessed anywhere via external import
                if accessed_members
                    .get(&export_name)
                    .is_some_and(|s| s.contains(&member.name))
                {
                    continue;
                }

                // Check if this member is accessed via `this.member` within the same file
                // (internal class usage — e.g., constructor sets this.label, methods use this.label)
                if matches!(
                    member.kind,
                    MemberKind::ClassMethod | MemberKind::ClassProperty
                ) && file_self_accesses.is_some_and(|accesses| accesses.contains(&member.name))
                {
                    continue;
                }

                // Skip decorated class members — decorators like @Column(), @ApiProperty(),
                // @Inject() etc. indicate runtime usage by frameworks (NestJS, TypeORM,
                // class-validator, class-transformer). These members are accessed
                // reflectively and should never be flagged as unused.
                if member.has_decorator {
                    continue;
                }

                // Skip React class component lifecycle methods — they are called by the
                // React runtime, not user code, so they should never be flagged as unused.
                // Also skip Angular lifecycle hooks (OnInit, OnDestroy, etc.).
                // The user allowlist extends these built-ins with framework-invoked names
                // contributed by plugins and top-level config (ag-Grid's `agInit`, etc.).
                if matches!(
                    member.kind,
                    MemberKind::ClassMethod | MemberKind::ClassProperty
                ) && (is_react_lifecycle_method(&member.name)
                    || is_angular_lifecycle_method(&member.name)
                    || user_class_member_allowlist.contains(member.name.as_str()))
                {
                    continue;
                }

                let (line, col) = byte_offset_to_line_col(
                    line_offsets_by_file,
                    module.file_id,
                    member.span.start,
                );

                // Check inline suppression
                let issue_kind = match member.kind {
                    MemberKind::EnumMember => IssueKind::UnusedEnumMember,
                    MemberKind::ClassMethod | MemberKind::ClassProperty => {
                        IssueKind::UnusedClassMember
                    }
                    MemberKind::NamespaceMember => unreachable!(),
                };
                if let Some(supps) = suppressions_by_file.get(&module.file_id)
                    && suppress::is_suppressed(supps, line, issue_kind)
                {
                    continue;
                }

                let unused = UnusedMember {
                    path: module.path.clone(),
                    parent_name: export_name.clone(),
                    member_name: member.name.clone(),
                    kind: member.kind,
                    line,
                    col,
                };

                match member.kind {
                    MemberKind::EnumMember => unused_enum_members.push(unused),
                    MemberKind::ClassMethod | MemberKind::ClassProperty => {
                        unused_class_members.push(unused);
                    }
                    MemberKind::NamespaceMember => unreachable!(),
                }
            }
        }
    }

    (unused_enum_members, unused_class_members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{DiscoveredFile, EntryPoint, EntryPointSource, FileId};
    use crate::extract::{
        ExportName, ImportInfo, ImportedName, MemberAccess, MemberInfo, MemberKind,
    };
    use crate::graph::{ExportSymbol, ModuleGraph, SymbolReference};
    use crate::resolve::{ResolveResult, ResolvedImport, ResolvedModule};
    use oxc_span::Span;
    use std::path::PathBuf;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test file counts are trivially small"
    )]
    fn build_graph(file_specs: &[(&str, bool)]) -> ModuleGraph {
        let files: Vec<DiscoveredFile> = file_specs
            .iter()
            .enumerate()
            .map(|(i, (path, _))| DiscoveredFile {
                id: FileId(i as u32),
                path: PathBuf::from(path),
                size_bytes: 0,
            })
            .collect();

        let entry_points: Vec<EntryPoint> = file_specs
            .iter()
            .filter(|(_, is_entry)| *is_entry)
            .map(|(path, _)| EntryPoint {
                path: PathBuf::from(path),
                source: EntryPointSource::ManualEntry,
            })
            .collect();

        let resolved_modules: Vec<ResolvedModule> = files
            .iter()
            .map(|f| ResolvedModule {
                file_id: f.id,
                path: f.path.clone(),
                ..Default::default()
            })
            .collect();

        ModuleGraph::build(&resolved_modules, &entry_points, &files)
    }

    fn make_member(name: &str, kind: MemberKind) -> MemberInfo {
        MemberInfo {
            name: name.to_string(),
            kind,
            span: Span::new(10, 20),
            has_decorator: false,
        }
    }

    fn make_export_with_members(
        name: &str,
        members: Vec<MemberInfo>,
        ref_from: Option<u32>,
    ) -> ExportSymbol {
        let references = ref_from
            .map(|from| {
                vec![SymbolReference {
                    from_file: FileId(from),
                    kind: crate::graph::ReferenceKind::NamedImport,
                    import_span: Span::new(0, 10),
                }]
            })
            .unwrap_or_default();
        ExportSymbol {
            name: ExportName::Named(name.to_string()),
            is_type_only: false,
            is_public: false,
            span: Span::new(0, 10),
            references,
            members,
        }
    }

    #[test]
    fn unused_members_empty_graph() {
        let graph = build_graph(&[]);

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn unused_enum_member_detected() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0), // referenced from entry
        )];

        // No member accesses at all — both should be unused
        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert_eq!(enum_members.len(), 2);
        assert!(class_members.is_empty());
        let names: FxHashSet<&str> = enum_members
            .iter()
            .map(|m| m.member_name.as_str())
            .collect();
        assert!(names.contains("Active"));
        assert!(names.contains("Inactive"));
    }

    #[test]
    fn accessed_enum_member_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        // Consumer accesses Status.Active
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "Status".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            member_accesses: vec![MemberAccess {
                object: "Status".to_string(),
                member: "Active".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Only Inactive should be unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Inactive");
    }

    #[test]
    fn whole_object_use_skips_all_members() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        // Consumer uses Object.values(Status) — whole object use
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "Status".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            whole_object_uses: vec!["Status".to_string()],
            ..Default::default()
        }];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn decorated_class_member_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/entity.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "User",
            vec![MemberInfo {
                name: "name".to_string(),
                kind: MemberKind::ClassProperty,
                span: Span::new(10, 20),
                has_decorator: true, // @Column() etc.
            }],
            Some(0),
        )];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(class_members.is_empty());
    }

    #[test]
    fn react_lifecycle_method_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/component.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyComponent",
            vec![
                make_member("render", MemberKind::ClassMethod),
                make_member("componentDidMount", MemberKind::ClassMethod),
                make_member("customMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Only customMethod should be flagged
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "customMethod");
    }

    #[test]
    fn angular_lifecycle_method_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/component.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "AppComponent",
            vec![
                make_member("ngOnInit", MemberKind::ClassMethod),
                make_member("ngOnDestroy", MemberKind::ClassMethod),
                make_member("myHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "myHelper");
    }

    #[test]
    fn user_class_member_allowlist_not_flagged() {
        // Third-party framework contract: library calls `agInit` and `refresh`
        // on the consumer class. The user allowlist (from config or a plugin)
        // extends the built-in Angular/React lifecycle check so these names are
        // treated as always-used. See issue #98 (ag-Grid `AgFrameworkComponent`).
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/renderer.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyRendererComponent",
            vec![
                make_member("agInit", MemberKind::ClassMethod),
                make_member("refresh", MemberKind::ClassMethod),
                make_member("customHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let mut allowlist: FxHashSet<&str> = FxHashSet::default();
        allowlist.insert("agInit");
        allowlist.insert("refresh");

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &allowlist,
        );
        assert_eq!(
            class_members.len(),
            1,
            "only customHelper should remain unused"
        );
        assert_eq!(class_members[0].member_name, "customHelper");
    }

    #[test]
    fn user_class_member_allowlist_does_not_affect_enums() {
        // The allowlist is scoped to class members; matching enum member names
        // must still be flagged as unused.
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/status.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("refresh", MemberKind::EnumMember)],
            Some(0),
        )];

        let mut allowlist: FxHashSet<&str> = FxHashSet::default();
        allowlist.insert("refresh");

        let (enum_members, _) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &allowlist,
        );
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "refresh");
    }

    #[test]
    fn this_member_access_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Service",
            vec![
                make_member("label", MemberKind::ClassProperty),
                make_member("unused_prop", MemberKind::ClassProperty),
            ],
            Some(0),
        )];

        // The service file itself accesses this.label
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(1), // same file as the service
            path: PathBuf::from("/src/service.ts"),
            member_accesses: vec![MemberAccess {
                object: "this".to_string(),
                member: "label".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Only unused_prop should be flagged (label is accessed via this)
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unused_prop");
    }

    #[test]
    fn unreferenced_export_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        // Export has members but NO references — whole export is dead, members skipped
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            None, // no references
        )];

        let (enum_members, _) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Member analysis skipped because export itself is unreferenced
        assert!(enum_members.is_empty());
    }

    #[test]
    fn unreachable_module_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/dead.ts", false)]);
        // Module 1 stays unreachable
        graph.modules[1].exports = vec![make_export_with_members(
            "DeadEnum",
            vec![make_member("X", MemberKind::EnumMember)],
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn entry_point_module_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true)]);
        graph.modules[0].exports = vec![make_export_with_members(
            "EntryEnum",
            vec![make_member("X", MemberKind::EnumMember)],
            None,
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn enum_member_kind_routed_to_enum_results() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].kind, MemberKind::EnumMember);
        assert!(class_members.is_empty());
    }

    #[test]
    fn class_member_kind_routed_to_class_results() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/class.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyClass",
            vec![
                make_member("myMethod", MemberKind::ClassMethod),
                make_member("myProp", MemberKind::ClassProperty),
            ],
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(enum_members.is_empty());
        assert_eq!(class_members.len(), 2);
        assert!(
            class_members
                .iter()
                .any(|m| m.kind == MemberKind::ClassMethod)
        );
        assert!(
            class_members
                .iter()
                .any(|m| m.kind == MemberKind::ClassProperty)
        );
    }

    #[test]
    fn instance_member_access_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyService",
            vec![
                make_member("greet", MemberKind::ClassMethod),
                make_member("unusedMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        // Consumer imports MyService and accesses greet via instance.
        // The visitor maps `svc.greet()` → `MyService.greet` at extraction time,
        // so the analysis layer sees it as a direct member access on the export name.
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./service".to_string(),
                    imported_name: ImportedName::Named("MyService".to_string()),
                    local_name: "MyService".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            member_accesses: vec![MemberAccess {
                // Already mapped by the visitor from `svc.greet()` → `MyService.greet`
                object: "MyService".to_string(),
                member: "greet".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Only unusedMethod should be flagged; greet is used via instance access
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unusedMethod");
    }

    #[test]
    fn this_access_does_not_skip_enum_members() {
        // `this.member` accesses only suppress class members, not enum members.
        // Enums don't have `this` — this test ensures the check is scoped to class kinds.
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Direction",
            vec![
                make_member("Up", MemberKind::EnumMember),
                make_member("Down", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        // File accesses this.Up — but for enum members, this should NOT suppress
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(1),
            path: PathBuf::from("/src/enums.ts"),
            member_accesses: vec![MemberAccess {
                object: "this".to_string(),
                member: "Up".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Both enum members should be flagged — `this` access doesn't apply to enums
        assert_eq!(enum_members.len(), 2);
    }

    #[test]
    fn mixed_enum_and_class_in_same_module() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/mixed.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![
            make_export_with_members(
                "Status",
                vec![make_member("Active", MemberKind::EnumMember)],
                Some(0),
            ),
            make_export_with_members(
                "Service",
                vec![make_member("doWork", MemberKind::ClassMethod)],
                Some(0),
            ),
        ];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].parent_name, "Status");
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].parent_name, "Service");
    }

    #[test]
    fn local_name_mapped_to_imported_name() {
        // import { Status as S } from './enums'
        // S.Active → should map "S" back to "Status" for member access matching
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "S".to_string(), // aliased
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            member_accesses: vec![MemberAccess {
                object: "S".to_string(), // uses local alias
                member: "Active".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // S.Active maps back to Status.Active, so only Inactive is unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Inactive");
    }

    #[test]
    fn default_import_maps_to_default_export() {
        // import MyEnum from './enums' → local "MyEnum", imported "default"
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "default",
            vec![
                make_member("X", MemberKind::EnumMember),
                make_member("Y", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Default,
                    local_name: "MyEnum".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            member_accesses: vec![MemberAccess {
                object: "MyEnum".to_string(),
                member: "X".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // MyEnum.X maps to default.X, so only Y is unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Y");
    }

    #[test]
    fn suppressed_enum_member_not_flagged() {
        use crate::suppress::{IssueKind, Suppression};

        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            Some(0),
        )];

        // Suppress on line 1 (byte offset 10 => line 1 with no offsets)
        let supps = vec![Suppression {
            line: 1,
            kind: Some(IssueKind::UnusedEnumMember),
        }];
        let mut suppressions: FxHashMap<FileId, &[Suppression]> = FxHashMap::default();
        suppressions.insert(FileId(1), &supps);

        let (enum_members, _) = find_unused_members(
            &graph,
            &[],
            &suppressions,
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(
            enum_members.is_empty(),
            "suppressed enum member should not be flagged"
        );
    }

    #[test]
    fn suppressed_class_member_not_flagged() {
        use crate::suppress::{IssueKind, Suppression};

        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Service",
            vec![make_member("doWork", MemberKind::ClassMethod)],
            Some(0),
        )];

        let supps = vec![Suppression {
            line: 1,
            kind: Some(IssueKind::UnusedClassMember),
        }];
        let mut suppressions: FxHashMap<FileId, &[Suppression]> = FxHashMap::default();
        suppressions.insert(FileId(1), &supps);

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &suppressions,
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(
            class_members.is_empty(),
            "suppressed class member should not be flagged"
        );
    }

    #[test]
    fn whole_object_use_via_aliased_import() {
        // import { Status as S } from './enums'
        // Object.values(S) → should map S back to Status and suppress all members
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("A", MemberKind::EnumMember),
                make_member("B", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "S".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            whole_object_uses: vec!["S".to_string()], // aliased local name
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Object.values(S) maps S→Status, so all members of Status should be considered used
        assert!(
            enum_members.is_empty(),
            "whole object use via alias should suppress all members"
        );
    }

    #[test]
    fn this_field_chained_access_not_flagged() {
        // `this.service = new MyService()` then `this.service.doWork()`
        // should recognize doWork as a used member of MyService.
        // The visitor emits MemberAccess { object: "MyService", member: "doWork" }
        // after resolving the `this.service` binding via instance_binding_names.
        let mut graph = build_graph(&[("/src/main.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyService",
            vec![
                make_member("doWork", MemberKind::ClassMethod),
                make_member("unusedMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        // Consumer imports MyService, stores in a field, and calls through it.
        // The visitor resolves `this.service.doWork()` → `MyService.doWork`.
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/main.ts"),
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./service".to_string(),
                    imported_name: ImportedName::Named("MyService".to_string()),
                    local_name: "MyService".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            member_accesses: vec![MemberAccess {
                // Already resolved by visitor from `this.service.doWork()` → `MyService.doWork`
                object: "MyService".to_string(),
                member: "doWork".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        // Only unusedMethod should be flagged; doWork is used via this.service.doWork()
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unusedMethod");
    }

    #[test]
    fn export_with_no_members_skipped() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/utils.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "helper",
            vec![], // no members
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
            &FxHashSet::default(),
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }
}
