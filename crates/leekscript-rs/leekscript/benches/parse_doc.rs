//! Parse throughput for typical LeekScript inputs (fixtures, sample project main, large sig stub).

use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use leekscript::{Version, parse_doc, parse_signature_doc};

fn parse_doc_criterion() -> Criterion {
    // Default 5s measurement window is too tight for 100 samples when each iteration parses
    // tens–hundreds of KiB (Criterion warns and skews stats).
    Criterion::default().measurement_time(Duration::from_secs(12))
}

fn bench_parse_doc_basic_fixture(c: &mut Criterion) {
    let src = include_str!("../testdata/basic.leek");
    c.bench_function("parse_doc/basic_fixture", |b| {
        b.iter(|| {
            let doc = parse_doc(black_box(src), Version::V4).expect("parse basic.leek");
            black_box(doc.root().text_len());
        })
    });
}

fn bench_parse_doc_ai_previous_main(c: &mut Criterion) {
    let src = include_str!("../../../../ai/previous/main.leek");
    c.bench_function("parse_doc/ai_previous_main", |b| {
        b.iter(|| {
            let doc = parse_doc(black_box(src), Version::V4).expect("parse ai/previous/main.leek");
            black_box(doc.root().text_len());
        })
    });
}

fn bench_parse_signature_std_functions(c: &mut Criterion) {
    let src = include_str!("../../../../sig/std.sig.functions.leek");
    c.bench_function("parse_signature_doc/std_functions_sig", |b| {
        b.iter(|| {
            let doc =
                parse_signature_doc(black_box(src), Version::V4).expect("parse std sig functions");
            black_box(doc.root().text_len());
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
