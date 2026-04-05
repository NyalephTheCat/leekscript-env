//! Parse throughput for typical LeekScript inputs (fixtures, sample project main, large sig stub).
//!
//! ## Criterion baselines (clean before/after comparisons)
//!
//! Save a reference once (e.g. on `main` before a change), then compare after edits:
//!
//! ```text
//! cargo bench -p leekscript --bench parse_doc -- --save-baseline main
//! cargo bench -p leekscript --bench parse_doc -- --baseline main
//! ```
//!
//! Or run `benches/compare_parse_doc_baseline.sh` from this crate directory.
//!
//! Benches use [`parse_doc_reusing_vec`](leekscript::parse_doc_reusing_vec) /
//! [`parse_signature_doc_reusing_vec`](leekscript::parse_signature_doc_reusing_vec) so the source
//! `Vec` capacity is recycled each iteration (still one memcpy per parse; avoids realloc).

use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use leekscript::{Version, parse_doc_reusing_vec, parse_signature_doc_reusing_vec};

fn parse_doc_criterion() -> Criterion {
    // Default 5s measurement window is too tight for 100 samples when each iteration parses
    // tens–hundreds of KiB (Criterion warns and skews stats).
    Criterion::default().measurement_time(Duration::from_secs(12))
}

fn bench_parse_doc_basic_fixture(c: &mut Criterion) {
    let src = include_str!("../testdata/basic.leek");
    let mut pool = Vec::with_capacity(src.len());
    c.bench_function("parse_doc/basic_fixture", |b| {
        b.iter(|| {
            let doc = parse_doc_reusing_vec(black_box(src), Version::V4, &mut pool)
                .expect("parse basic.leek");
            let n = black_box(doc.root().text_len());
            pool = doc.into_bytes();
            black_box(n);
        })
    });
}

fn bench_parse_doc_ai_previous_main(c: &mut Criterion) {
    let src = include_str!("../../../../ai/previous/main.leek");
    let mut pool = Vec::with_capacity(src.len());
    c.bench_function("parse_doc/ai_previous_main", |b| {
        b.iter(|| {
            let doc = parse_doc_reusing_vec(black_box(src), Version::V4, &mut pool)
                .expect("parse ai/previous/main.leek");
            let n = black_box(doc.root().text_len());
            pool = doc.into_bytes();
            black_box(n);
        })
    });
}

fn bench_parse_signature_std_functions(c: &mut Criterion) {
    let src = concat!(
        include_str!("../../../../sig/core/stdlib.sig.functions.leek"),
        "\n",
        include_str!("../../../../sig/leekwars/leekwars.sig.functions.leek"),
    );
    let mut pool = Vec::with_capacity(src.len());
    c.bench_function("parse_signature_doc/std_functions_sig", |b| {
        b.iter(|| {
            let doc = parse_signature_doc_reusing_vec(black_box(src), Version::V4, &mut pool)
                .expect("parse std sig functions");
            let n = black_box(doc.root().text_len());
            pool = doc.into_bytes();
            black_box(n);
        })
    });
}

criterion_group! {
    name = benches;
    config = parse_doc_criterion();
    targets =
        bench_parse_doc_basic_fixture,
        bench_parse_doc_ai_previous_main,
        bench_parse_signature_std_functions
}
criterion_main!(benches);
