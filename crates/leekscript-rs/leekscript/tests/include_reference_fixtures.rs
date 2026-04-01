//! Include graph loading against `leekscript-java/src/test/resources` (shared fixture tree).

use leekscript::{
    IncludeLimits, IncludeLoadError, Version, load_project_with_includes,
    load_project_with_includes_limited,
};
use std::path::{Path, PathBuf};

fn reference_resources_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("leekscript-java")
        .join("src")
        .join("test")
        .join("resources")
}

#[test]
fn loads_multiple_includes_dedupes_distinct_files() {
    let root = reference_resources_root();
    let p = load_project_with_includes(&root, Path::new("ai/multiple_includes.leek"), Version::V4)
        .unwrap();
    assert_eq!(
        p.files.len(),
        2,
        "entry + bonjour once despite three includes"
    );
    assert!(p.files[0].path.ends_with("multiple_includes.leek"));
    assert!(p.files[1].path.ends_with("bonjour.leek"));
}

#[test]
fn loads_include_subfolder_path() {
    let root = reference_resources_root();
    let p =
        load_project_with_includes(&root, Path::new("ai/include_sub.leek"), Version::V4).unwrap();
    assert_eq!(p.files.len(), 2);
    assert!(p.files[0].path.ends_with("include_sub.leek"));
    assert!(p.files[1].path.ends_with("sub.leek"));
}

#[test]
fn loads_include_parent_dot_dot() {
    // Same `../` rule as `ai/subfolder/include_parent.leek` → `../french.leek` in the fixture tree.
    // `french.leek` is not parsed by this crate’s V4 grammar yet; use minimal V4 fixtures here.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("include_dotdot");
    let p = load_project_with_includes(&root, Path::new("sub/includer.leek"), Version::V4).unwrap();
    assert_eq!(p.files.len(), 2);
    assert!(p.files[0].path.ends_with("includer.leek"));
    assert!(p.files[1].path.ends_with("target.leek"));
}

#[test]
fn loads_include_chain_array_keys_and_library() {
    let root = reference_resources_root();
    let p = load_project_with_includes(&root, Path::new("ai/include_multiple.leek"), Version::V4)
        .unwrap();
    assert_eq!(p.files.len(), 3);
    assert!(p.files[0].path.ends_with("include_multiple.leek"));
    assert!(p.files[1].path.ends_with("array_keys.leek"));
    assert!(p.files[2].path.ends_with("library.leek"));
}

#[test]
fn custom_include_limit_stops_transitive_load() {
    let tmp = std::env::temp_dir().join("leekscript_include_cap_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("a.leek"), "include(\"b.leek\");\n").unwrap();
    std::fs::write(tmp.join("b.leek"), "return 1;\n").unwrap();
    let err = load_project_with_includes_limited(
        &tmp,
        Path::new("a.leek"),
        Version::V4,
        IncludeLimits {
            max_distinct_files: 0,
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            IncludeLoadError::TooManyIncludes {
                max_distinct_files: 0
            }
        ),
        "expected TooManyIncludes {{ max_distinct_files: 0 }}, got {err:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn missing_include_errors() {
    let tmp = std::env::temp_dir().join("leekscript_include_test_fake.leek");
    std::fs::write(&tmp, "include(\"no_such_file_xyz.leek\");\n").unwrap();
    let res =
        load_project_with_includes(tmp.parent().unwrap(), tmp.file_name().unwrap(), Version::V4);
    std::fs::remove_file(&tmp).ok();
    let e = res.unwrap_err();
    assert!(
        matches!(e, IncludeLoadError::NotFound { .. }),
        "expected NotFound, got {e:?}"
    );
}
