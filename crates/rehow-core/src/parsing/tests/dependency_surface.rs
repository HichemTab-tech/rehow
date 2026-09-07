use rowan::ast::AstNode;

fn dependency_adapter(text: &str) -> (usize, usize) {
    let parse = yaml_edit::YamlFile::parse(text);
    let error_count = parse.positioned_errors().len();
    let tree = parse.tree();
    let syntax = tree.syntax();
    let mut syntax_visits = usize::from(syntax.kind() == yaml_edit::SyntaxKind::ROOT);
    let _ = (syntax.text(), syntax.text_range(), syntax.parent());
    if let Some(element) = syntax.first_child_or_token() {
        syntax_visits += 1;
        let _ = (element.kind(), element.next_sibling_or_token());
        match element {
            rowan::NodeOrToken::Node(node) => {
                let _ = (node.text(), node.text_range(), node.parent());
            }
            rowan::NodeOrToken::Token(token) => {
                let _ = (token.text(), token.text_range(), token.parent());
            }
        }
    }
    if let Some(node) = syntax
        .children()
        .flat_map(|node| node.children())
        .flat_map(|node| node.children())
        .find(|node| node.kind() == yaml_edit::SyntaxKind::SCALAR)
    {
        let scalar = yaml_edit::Scalar::cast(node).unwrap();
        let _ = (scalar.syntax(), scalar.value(), scalar.as_string());
    }
    let mut parser = saphyr_parser::Parser::new_from_str(text).keep_tags(false);
    while let Some(event) = parser.next_event() {
        let _event_was_ok = event.is_ok();
        syntax_visits += 1;
    }
    (error_count, syntax_visits)
}

#[test]
fn exact_allowed_dependency_adapter_compiles_and_visits_both_parsers() {
    let (errors, visits) = dependency_adapter("a: b\n");
    assert_eq!(errors, 0);
    assert!(visits > 1);
}

#[test]
fn production_dependency_imports_and_calls_match_the_frozen_allowlist() {
    let presentation = include_str!("../presentation.rs");
    let semantic = include_str!("../semantic.rs");
    let production_sources = production_parser_sources();
    let production = production_sources.join("\n");
    let exact_imports = [
        "use rowan::ast::AstNode;",
        "use rowan::{NodeOrToken, SyntaxNode};",
        "use yaml_edit::{Lang, ParseErrorKind, Scalar, SyntaxKind, YamlFile};",
        "use saphyr_parser::{Event, Parser, ScalarStyle as SaphyrStyle, Tag};",
    ];
    for import in exact_imports {
        assert_eq!(production.matches(import).count(), 1, "import {import}");
    }
    for prefix in ["use yaml_edit", "use rowan", "use saphyr_parser"] {
        assert_eq!(
            production
                .lines()
                .filter(|line| line.starts_with(prefix))
                .count(),
            if prefix == "use rowan" { 2 } else { 1 },
            "dependency imports must be exhaustively enumerated"
        );
    }

    let presentation_production = &presentation[..presentation
        .find("#[cfg(test)]\npub(super) fn exhaustive_kind_outcomes")
        .unwrap()];
    for (call, count) in [
        ("YamlFile::parse", 1),
        (".positioned_errors()", 2),
        (".tree()", 1),
        (".syntax()", 1),
        (".kind()", 23),
        (".text()", 2),
        (".text_range()", 2),
        (".parent()", 1),
        (".first_child_or_token()", 1),
        (".next_sibling_or_token()", 1),
        ("Scalar::cast", 1),
        (".value()", 1),
        (".as_string()", 1),
    ] {
        assert_eq!(
            presentation_production.matches(call).count(),
            count,
            "exact yaml-edit/rowan call count for {call}"
        );
    }
    // Frozen from every public inherent method on rowan 0.16.1's typed and
    // raw SyntaxNode/SyntaxToken/SyntaxElement, child/preorder cursors,
    // NodeOrToken, TokenAtOffset, WalkEvent, SyntaxNodePtr, and AstPtr in
    // api.rs, cursor.rs, utility_types.rs, and ast.rs. Methods shared with
    // ordinary iterators (`next`, `map`) are reachable only through a denied
    // rowan constructor below. Any direct structural/navigation/conversion
    // call other than ADR 0013's three iterative operations must stay absent.
    for (method, expected) in [
        ("ancestors", 0),
        ("children", 0),
        ("children_with_tokens", 0),
        ("first_child", 0),
        ("first_child_by_kind", 0),
        ("last_child", 0),
        ("first_child_or_token", 1),
        ("first_child_or_token_by_kind", 0),
        ("last_child_or_token", 0),
        ("to_next_sibling", 0),
        ("next_sibling", 0),
        ("next_sibling_by_kind", 0),
        ("prev_sibling", 0),
        ("next_sibling_or_token", 1),
        ("to_next_sibling_or_token", 0),
        ("next_sibling_or_token_by_kind", 0),
        ("prev_sibling_or_token", 0),
        ("first_token", 0),
        ("last_token", 0),
        ("siblings", 0),
        ("siblings_with_tokens", 0),
        ("descendants", 0),
        ("descendants_with_tokens", 0),
        ("preorder", 0),
        ("preorder_with_tokens", 0),
        ("token_at_offset", 0),
        ("covering_element", 0),
        ("child_or_token_at_range", 0),
        ("clone_subtree", 0),
        ("clone_for_update", 0),
        ("parent_ancestors", 0),
        ("next_token", 0),
        ("prev_token", 0),
        ("by_kind", 0),
        ("skip_subtree", 0),
        ("detach", 0),
        ("green", 0),
        ("index", 0),
        ("is_mutable", 0),
        ("replace_with", 0),
        ("splice_children", 0),
        ("into_node", 0),
        ("into_token", 0),
        ("as_node", 0),
        ("as_token", 0),
        ("right_biased", 0),
        ("left_biased", 0),
        ("to_node", 0),
        ("try_to_node", 0),
        ("syntax_node_ptr", 0),
        ("can_cast", 0),
        ("cast", 0),
    ] {
        let call = format!(".{method}(");
        assert_eq!(
            presentation_production.matches(&call).count(),
            expected,
            "exact pinned-rowan navigation call count for {call}"
        );
    }
    assert_eq!(
        presentation_production.matches("rowan::").count(),
        4,
        "only the two exact imports and two audited type paths may use the rowan namespace"
    );
    assert_eq!(
        presentation_production
            .matches("rowan::SyntaxToken")
            .count(),
        1
    );
    assert_eq!(
        presentation_production.matches("rowan::TextRange").count(),
        1
    );
    assert_eq!(
        presentation_production.matches("yaml_edit::").count(),
        1,
        "only the exact yaml-edit import may use the namespace"
    );
    assert_eq!(
        qualified_calls(presentation_production, "YamlFile"),
        ["parse"]
    );
    assert_eq!(qualified_calls(presentation_production, "Scalar"), ["cast"]);
    let semantic_parse = &semantic[semantic.find("pub(super) fn parse(").unwrap()
        ..semantic.find("impl SemanticTrace").unwrap()];
    for call in ["Parser::new_from_str", ".keep_tags(false)", ".next_event()"] {
        assert_eq!(
            semantic_parse.matches(call).count(),
            1,
            "exact Saphyr call {call}"
        );
    }
    assert_eq!(qualified_calls(semantic_parse, "Parser"), ["new_from_str"]);
    for (namespace, expected) in [("rowan::", 4), ("yaml_edit::", 1), ("saphyr_parser::", 1)] {
        assert_eq!(
            production.matches(namespace).count(),
            expected,
            "exact aggregate production namespace count for {namespace}"
        );
    }
    for source in [
        &production_sources[0],
        &production_sources[1],
        &production_sources[3],
        &production_sources[4],
        &production_sources[5],
    ] {
        assert_eq!(source.matches("rowan::").count(), 0);
        assert_eq!(source.matches("yaml_edit::").count(), 0);
    }
    for source in [
        &production_sources[0],
        &production_sources[1],
        &production_sources[2],
        &production_sources[4],
        &production_sources[5],
    ] {
        assert_eq!(source.matches("saphyr_parser::").count(), 0);
    }
    assert_eq!(qualified_calls(&production, "YamlFile"), ["parse"]);
    assert_eq!(qualified_calls(&production, "Scalar"), ["cast"]);
    assert_eq!(qualified_calls(&production, "Parser"), ["new_from_str"]);
    for (suffix, exact) in [
        ("::parse(", "YamlFile::parse("),
        ("::cast(", "Scalar::cast("),
        ("::new_from_str(", "Parser::new_from_str("),
    ] {
        let expected = if suffix == "::parse(" { 2 } else { 1 };
        assert_eq!(
            production.matches(suffix).count(),
            expected,
            "associated suffix {suffix}"
        );
        assert_eq!(
            production.matches(exact).count(),
            1,
            "exact associated call {exact}"
        );
    }
    assert_eq!(production.matches("semantic::parse(").count(), 1);
    assert!(associated_calls_are_exact(&production));
    for approved_dot_method in [
        "positioned_errors",
        "tree",
        "syntax",
        "kind",
        "text",
        "text_range",
        "parent",
        "first_child_or_token",
        "next_sibling_or_token",
        "value",
        "as_string",
        "keep_tags",
        "next_event",
    ] {
        assert_eq!(
            production
                .matches(&format!("::{approved_dot_method}("))
                .count(),
            0,
            "approved dot call must not use qualified/UFCS spelling"
        );
    }

    assert_no_forbidden_dependency_method(&production);
    for bypass in [
        "SyntaxNode::clone_for_update(root)",
        "Alias::clone_for_update(root)",
        "NodeOrToken::into_node(element)",
        "SyntaxNode::descendants(root)",
        "<Scalar as AstNode>::cast(node)",
        "Alias::parse(source)",
        "<YamlFile>::parse(source)",
        "Alias::new_from_str(source)",
        "<Parser>::new_from_str(source)",
        "rowan::SyntaxNode::descendants(root)",
        "fn reconcile_like() { yaml_edit::YamlFile::parse(\"x\"); }",
    ] {
        assert!(
            has_forbidden_dependency_method(bypass)
                || has_non_normative_associated_call(bypass)
                || bypass.contains("rowan::")
                || bypass.contains("yaml_edit::")
                || bypass.contains("saphyr_parser::"),
            "audit must detect dependency bypass {bypass}"
        );
    }
    for addition in [
        "Alias::parse(source);",
        "<YamlFile>::parse(source);",
        "Alias::cast(node);",
        "<Scalar as AstNode>::cast(node);",
        "Alias::new_from_str(source);",
        "<Parser>::new_from_str(source);",
    ] {
        assert!(!associated_calls_are_exact(&format!(
            "{production}\n{addition}"
        )));
    }
    for (normative, replacement) in [
        ("YamlFile::parse(", "Alias::parse("),
        ("Scalar::cast(", "<Scalar as AstNode>::cast("),
        ("Parser::new_from_str(", "Alias::new_from_str("),
    ] {
        let replaced = production.replacen(normative, replacement, 1);
        assert!(!associated_calls_are_exact(&replaced));
    }
    assert!(production_sources[4].contains("fn is_noncharacter(character: char)"));
    assert!(!production_sources[4].contains(".descendants("));
    let scalar_post_inline_cfg_probe = format!("{}\nvalue.descendants();", production_sources[4]);
    assert!(has_forbidden_dependency_method(
        &scalar_post_inline_cfg_probe
    ));
    assert_eq!(
        presentation.matches("YamlFile::parse").count(),
        3,
        "one production call and two cfg(test) dependency-return controls"
    );
    assert_eq!(
        semantic.matches("Parser::new_from_str").count(),
        2,
        "one production call and one cfg(test) copied-return control"
    );
}

fn forbidden_dependency_methods() -> &'static [&'static str] {
    &[
        "ancestors",
        "children",
        "children_with_tokens",
        "first_child",
        "first_child_by_kind",
        "last_child",
        "first_child_or_token_by_kind",
        "last_child_or_token",
        "to_next_sibling",
        "next_sibling",
        "next_sibling_by_kind",
        "prev_sibling",
        "to_next_sibling_or_token",
        "next_sibling_or_token_by_kind",
        "prev_sibling_or_token",
        "first_token",
        "last_token",
        "siblings",
        "siblings_with_tokens",
        "descendants",
        "descendants_with_tokens",
        "preorder",
        "preorder_with_tokens",
        "token_at_offset",
        "covering_element",
        "child_or_token_at_range",
        "clone_subtree",
        "clone_for_update",
        "parent_ancestors",
        "next_token",
        "prev_token",
        "by_kind",
        "skip_subtree",
        "detach",
        "green",
        "index",
        "is_mutable",
        "replace_with",
        "splice_children",
        "into_node",
        "into_token",
        "as_node",
        "as_token",
        "right_biased",
        "left_biased",
        "to_node",
        "try_to_node",
        "syntax_node_ptr",
        "can_cast",
        "read",
        "from_str_relaxed",
        "from_path",
        "parse_relaxed",
        "validate",
        "get_path",
        "as_mapping",
        "as_sequence",
        "as_scalar",
        "to_string_with",
        "to_yaml",
        "serialize",
        "set_value",
        "set_tag",
        "load",
        "peek",
    ]
}

fn has_forbidden_dependency_method(source: &str) -> bool {
    forbidden_dependency_methods().iter().any(|method| {
        source.contains(&format!(".{method}(")) || source.contains(&format!("::{method}("))
    }) || source.contains("::cast(") && !source.contains("Scalar::cast(")
}

fn has_non_normative_associated_call(source: &str) -> bool {
    [
        ("::parse(", "YamlFile::parse("),
        ("::cast(", "Scalar::cast("),
        ("::new_from_str(", "Parser::new_from_str("),
    ]
    .iter()
    .any(|(suffix, exact)| source.matches(suffix).count() != source.matches(exact).count())
}

fn associated_calls_are_exact(source: &str) -> bool {
    source.matches("::parse(").count() == 2
        && source.matches("YamlFile::parse(").count() == 1
        && source.matches("semantic::parse(").count() == 1
        && source.matches("::cast(").count() == 1
        && source.matches("Scalar::cast(").count() == 1
        && source.matches("::new_from_str(").count() == 1
        && source.matches("Parser::new_from_str(").count() == 1
}

fn assert_no_forbidden_dependency_method(source: &str) {
    for method in forbidden_dependency_methods() {
        for separator in ['.', ':'] {
            let call = if separator == '.' {
                format!(".{method}(")
            } else {
                format!("::{method}(")
            };
            assert_eq!(
                source.matches(&call).count(),
                0,
                "forbidden aggregate production dependency spelling {call}"
            );
        }
    }
    assert_eq!(source.matches("::cast(").count(), 1);
    assert_eq!(source.matches("Scalar::cast(").count(), 1);
}

fn production_parser_sources() -> [String; 6] {
    let module = include_str!("../mod.rs");
    let source = include_str!("../source.rs");
    let presentation = include_str!("../presentation.rs");
    let semantic = include_str!("../semantic.rs");
    let scalar = include_str!("../scalar.rs");
    let reconcile = include_str!("../reconcile.rs");
    [
        module[..module.find("#[cfg(test)]\nfn combined_stage_8").unwrap()].to_owned(),
        source.to_owned(),
        format!(
            "{}{}",
            &presentation[..presentation
                .find("#[cfg(test)]\npub(super) fn tail_descriptor_layout")
                .unwrap()],
            &presentation[presentation.find("pub(super) struct Presentation").unwrap()
                ..presentation
                    .find("#[cfg(test)]\npub(super) fn exhaustive_kind_outcomes")
                    .unwrap()]
        ),
        format!(
            "{}{}",
            &semantic[..semantic.find("#[cfg(test)]\nfn fold_for_test").unwrap()],
            &semantic[semantic.find("fn fold_inner(").unwrap()
                ..semantic
                    .find("#[cfg(test)]\npub(super) fn borrowed_text_limit_for_test")
                    .unwrap()]
        ),
        scalar[..scalar
            .find("#[cfg(test)]\npub(super) fn grammar_form_for_test")
            .unwrap()]
            .to_owned(),
        format!(
            "{}{}",
            &reconcile[..reconcile
                .find("#[cfg(test)]\npub(super) fn reset_pipeline_observation_for_test")
                .unwrap()],
            &reconcile[reconcile.find("fn prepare(").unwrap()..]
        ),
    ]
}

fn qualified_calls<'a>(source: &'a str, qualifier: &str) -> Vec<&'a str> {
    let prefix = format!("{qualifier}::");
    let mut calls = Vec::new();
    for suffix in source.split(&prefix).skip(1) {
        let name_len = suffix
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let (name, tail) = suffix.split_at(name_len);
        if tail.trim_start().starts_with('(') {
            calls.push(name);
        }
    }
    calls
}

#[test]
fn production_parser_source_rejects_the_complete_forbidden_surface() {
    let sources = [
        include_str!("../mod.rs"),
        include_str!("../presentation.rs"),
        include_str!("../semantic.rs"),
        include_str!("../scalar.rs"),
        include_str!("../reconcile.rs"),
        include_str!("../source.rs"),
    ];
    for source in sources {
        for forbidden in [
            "unsafe fn",
            "unsafe {",
            "YamlFile::read",
            "YamlFile::from_str_relaxed",
            "YamlFile::from_path",
            "YamlFile::parse_relaxed",
            ".validate(",
            ".get_path(",
            ".as_mapping(",
            ".as_sequence(",
            ".as_scalar(",
            ".to_string_with",
            ".to_yaml(",
            ".serialize(",
            "YamlWriter",
            "YamlBuilder",
            "GreenNode",
            "GreenToken",
            "GreenNodeBuilder",
            "splice_children",
            "replace_with(",
            "set_value(",
            "set_tag(",
            "Parser::load",
            ".peek(",
            "saphyr_parser::Scanner",
            "saphyr_parser::ScanError",
            "saphyr_parser::Marker",
            "saphyr_parser::Span",
            "saphyr_parser::Input",
            "saphyr_parser::Receiver",
            "source_policy_precheck",
            "serde_yaml",
            "fn lexer",
            "fn lex_",
            "fn tokenize",
            "fn parse_token",
        ] {
            assert!(
                !source.contains(forbidden),
                "forbidden production surface {forbidden}"
            );
        }
    }
}

#[test]
fn retained_records_have_no_dependency_handles_and_dead_code_reason_is_exact() {
    let presentation = include_str!("../presentation.rs");
    let presentation_record =
        &presentation[presentation.find("pub(super) struct Presentation").unwrap()
            ..presentation.find("impl Presentation").unwrap()];
    let semantic = include_str!("../semantic.rs");
    let semantic_record = &semantic[semantic.find("pub(super) struct SemanticTrace").unwrap()
        ..semantic.find("struct Pending").unwrap()];
    let reconcile = include_str!("../reconcile.rs");
    let artifact_records = &reconcile[reconcile.find("pub(crate) struct ReconciledNode").unwrap()
        ..reconcile.find("impl ReconciledNode").unwrap()];
    let deferred_records = &reconcile[reconcile.find("struct DraftNode").unwrap()
        ..reconcile.find("pub(super) fn reconcile(").unwrap()];
    for records in [
        presentation_record,
        semantic_record,
        artifact_records,
        deferred_records,
    ] {
        for forbidden in [
            "YamlFile",
            "yaml_edit::",
            "SyntaxNode",
            "SyntaxToken",
            "NodeOrToken",
            "rowan::",
            "Parser<",
            "saphyr_parser::",
            "Event<",
            "Marker",
            "Span",
            "Cow<",
        ] {
            assert!(
                !records.contains(forbidden),
                "retained dependency type {forbidden}"
            );
        }
    }
    let crate_root = include_str!("../../lib.rs");
    assert!(crate_root.contains(
        "#[allow(\n    dead_code,\n    reason = \"Phase 1C parser is consumed beginning in Phase 1D\"\n)]\npub(crate) mod parsing;"
    ));
    assert_eq!(crate_root.matches("allow(\n    dead_code").count(), 1);
    for source in sources_without_crate_root() {
        assert!(!source.contains("allow(dead_code"));
        assert!(!source.contains("allow(\n    dead_code"));
    }
}

fn sources_without_crate_root() -> [&'static str; 6] {
    [
        include_str!("../mod.rs"),
        include_str!("../presentation.rs"),
        include_str!("../semantic.rs"),
        include_str!("../scalar.rs"),
        include_str!("../reconcile.rs"),
        include_str!("../source.rs"),
    ]
}

#[test]
fn traversal_is_iterative_and_test_controls_are_structurally_test_only() {
    let presentation = include_str!("../presentation.rs");
    assert!(presentation.contains("while let Some(element) = current.take()"));
    assert!(!presentation.contains("fn visit_node"));
    assert!(!presentation.contains("fn walk"));
    let reconcile = include_str!("../reconcile.rs");
    assert!(!reconcile.contains("fn visit_node"));
    assert!(!reconcile.contains("fn walk"));
    let semantic = include_str!("../semantic.rs");
    assert!(semantic.contains("#[cfg(test)]\nstruct FoldControl"));
    assert!(semantic.contains("#[cfg(test)] control: FoldControl"));
    assert!(!include_str!("../mod.rs").contains("ParseTestControl"));
    assert!(!presentation.contains("forced_issue"));
    assert!(!semantic.contains("TailReducer"));
    for control in [
        "FoldControl",
        "FoldFault",
        "ReconcileFault",
        "NodeMismatchDimension",
    ] {
        let declaration = if control.starts_with("Fold") {
            format!("struct {control}")
        } else {
            format!("pub(super) enum {control}")
        };
        let source = if control.starts_with("Fold") {
            semantic
        } else {
            reconcile
        };
        let at = source.find(&declaration).unwrap();
        assert!(
            source[at.saturating_sub(96)..at].contains("#[cfg(test)]"),
            "{control} must have a structurally cfg(test) declaration"
        );
    }
}

#[test]
fn crate_surface_has_no_parser_artifact_or_test_control_reexports() {
    let root = include_str!("../../lib.rs");
    assert!(root.contains("pub(crate) mod parsing;"));
    for forbidden in [
        "pub mod parsing",
        "pub use parsing",
        "ParsedYamlDocument",
        "ParsedGuideSource",
        "ParsedYamlArtifact",
        "ParseFailure",
        "YamlByteOffset",
        "YamlByteRange",
        "FileByteOffset",
        "FileByteRange",
        "CommentRecord",
        "CommentOwner",
        "RoundTripReport",
        "RoundTripIssue",
        "ParseTestControl",
        "ReconcileFault",
        "FoldControl",
    ] {
        assert!(
            !root.contains(forbidden),
            "public crate root contains {forbidden}"
        );
    }
}
