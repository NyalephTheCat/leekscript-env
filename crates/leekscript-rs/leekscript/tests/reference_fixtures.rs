use leekscript::{Version, parse_doc};

fn parse_reference_fixture(rel: &str, version: Version) {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..");
    let path = repo_root
        .join("leek-wars-generator")
        .join("leekscript")
        .join("src")
        .join("test")
        .join("resources")
        .join(rel);
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_doc(&src, version)
        .unwrap_or_else(|e| panic!("parse {rel}:\n{}", e.format_with_source(&src)));
}

#[test]
fn parses_reference_fixtures_subset_v4() {
    // Start small and expand as grammar coverage improves.
    // Same `.leek` files as under `leek-wars-generator/leekscript/src/test/resources`.
    parse_reference_fixture("ai/code/trivial.leek", Version::V4);
    parse_reference_fixture("ai/code/assignments.leek", Version::V4);
    parse_reference_fixture("ai/code/return_in_function.leek", Version::V4);
    parse_reference_fixture("ai/code/strings.leek", Version::V4);
    parse_reference_fixture("ai/code/array.leek", Version::VNext);
    parse_reference_fixture("ai/code/break_and_continue.leek", Version::VNext);
    parse_reference_fixture("ai/code/match.leek", Version::VNext);
    parse_reference_fixture("ai/code/dynamic_operators.leek", Version::V4);
    parse_reference_fixture("ai/code/pow5.leek", Version::V4);

    parse_reference_fixture("ai/code/primes_typed.leek", Version::V4);
    parse_reference_fixture("ai/code/classes_multiple.leek", Version::V4);

    parse_reference_fixture("ai/code/product_coproduct.leek", Version::V4);
    parse_reference_fixture("ai/code/fold_right.leek", Version::V4);
}
