//! Merging a loaded include graph into one source file (`merge_included_sources_to_single_file`).

use leekscript::{
    Version, load_project_with_includes, merge_included_sources_to_single_file,
    merge_included_sources_to_single_file_mapped, parse_signature_doc, prepend_signatures_to_merged,
};
use std::path::Path;

fn tmp_merge_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "leekscript_merge_test_{name}_{}",
        std::process::id()
    ))
}

#[test]
fn merge_triple_include_same_file_once() {
    let root = tmp_merge_root("triple");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lib.leek"), "return 1;\n").unwrap();
    std::fs::write(
        root.join("main.leek"),
        "include(\"lib.leek\");\ninclude(\"lib.leek\");\ninclude(\"lib.leek\");\n",
    )
    .unwrap();

    let p = load_project_with_includes(&root, Path::new("main.leek"), Version::V4).unwrap();
    let s = merge_included_sources_to_single_file(&root, &p).unwrap();
    assert_eq!(
        s.matches("return 1").count(),
        1,
        "body should appear once: {s:?}"
    );
    assert_eq!(s.matches("already merged:").count(), 2);
    assert_eq!(s.matches("begin lib.leek").count(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn merge_diamond_common_once() {
    let root = tmp_merge_root("diamond");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("common.leek"), "1;\n").unwrap();
    std::fs::write(root.join("b.leek"), "include(\"common.leek\");\n").unwrap();
    std::fs::write(root.join("c.leek"), "include(\"common.leek\");\n2;\n").unwrap();
    std::fs::write(
        root.join("a.leek"),
        "include(\"b.leek\");\ninclude(\"c.leek\");\n",
    )
    .unwrap();

    let p = load_project_with_includes(&root, Path::new("a.leek"), Version::V4).unwrap();
    let s = merge_included_sources_to_single_file(&root, &p).unwrap();
    assert_eq!(
        s.matches("1;").count(),
        1,
        "common.leek should be inlined once: {s}"
    );
    assert!(
        s.contains("already merged:"),
        "second include of common should be skipped: {s}"
    );
    assert!(s.contains("2;"), "c.leek body should appear: {s}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn merge_source_mapping_points_at_original_file() {
    let root = tmp_merge_root("mapping");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lib.leek"), "class C {}\n").unwrap();
    std::fs::write(
        root.join("main.leek"),
        "include(\"lib.leek\");\nvar x = 1;\n",
    )
    .unwrap();

    let p = load_project_with_includes(&root, Path::new("main.leek"), Version::V4).unwrap();
    let (merged, map) = merge_included_sources_to_single_file_mapped(&root, &p).unwrap();
    let pos = merged.find("class").expect("class keyword in merged") as u32;
    let sm = map
        .span_at_merged_offset(pos)
        .expect("mapping for lib body");
    assert!(
        sm.path.ends_with("lib.leek"),
        "expected lib.leek, got {:?}",
        sm.path
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn prepend_signatures_shifts_mapping_and_inserts_prelude() {
    let root = tmp_merge_root("sig_prepend");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let sig = root.join("stdlib.leek");
    std::fs::write(&sig, "function abs(integer x) {}\n").unwrap();
    std::fs::write(root.join("main.leek"), "var y = abs(1);\n").unwrap();

    let p = load_project_with_includes(&root, Path::new("main.leek"), Version::V4).unwrap();
    let (merged, map) = merge_included_sources_to_single_file_mapped(&root, &p).unwrap();
    let (combined, full_map) =
        prepend_signatures_to_merged(Version::V4, &[sig.clone()], &merged, map).unwrap();

    assert!(combined.starts_with("function abs"));
    assert!(combined.contains("var y = abs"));

    let pos_stdlib = combined.find("function abs").expect("stdlib") as u32;
    let sm = full_map
        .span_at_merged_offset(pos_stdlib)
        .expect("stdlib mapping");
    assert!(
        sm.path.ends_with("stdlib.leek"),
        "expected stdlib.leek, got {:?}",
        sm.path
    );

    let pos_main = combined.find("var y").expect("main") as u32;
    let sm2 = full_map
        .span_at_merged_offset(pos_main)
        .expect("main mapping");
    assert!(
        sm2.path.ends_with("main.leek"),
        "expected main.leek, got {:?}",
        sm2.path
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn prepend_signatures_expands_top_level_includes() {
    let root = tmp_merge_root("sig_include");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let bundle = root.join("bundle.sig.leek");
    let frag = root.join("fragment.sig.leek");
    std::fs::write(&frag, "function tiny() => integer;\n").unwrap();
    std::fs::write(
        &bundle,
        "include(\"fragment.sig.leek\");\nfunction other() => integer;\n",
    )
    .unwrap();
    std::fs::write(root.join("main.leek"), "var y = 1;\n").unwrap();

    let p = load_project_with_includes(&root, Path::new("main.leek"), Version::V4).unwrap();
    let (merged, map) = merge_included_sources_to_single_file_mapped(&root, &p).unwrap();
    let (combined, _) =
        prepend_signatures_to_merged(Version::V4, &[bundle.clone()], &merged, map).unwrap();

    assert!(
        combined.contains("function tiny()"),
        "included fragment should be in prelude: {combined}"
    );
    assert!(
        combined.contains("function other()"),
        "bundle decl after include should remain: {combined}"
    );
    assert!(combined.contains("var y"), "main should follow prelude");

    parse_signature_doc(&combined, Version::V4).expect("parse merged prelude+main");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn merge_matches_java_fixture_multiple_includes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("leek-wars-generator")
        .join("leekscript")
        .join("src")
        .join("test")
        .join("resources");
    let p = load_project_with_includes(&root, Path::new("ai/multiple_includes.leek"), Version::V4)
        .unwrap();
    let s = merge_included_sources_to_single_file(&root, &p).unwrap();
    assert_eq!(s.matches("return 'bonjour'").count(), 1);
    assert_eq!(s.matches("already merged:").count(), 2);
}
