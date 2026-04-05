#![no_main]

//! Single harness: **strict parse, signature parse, recovery parse → semantics → format → VM**
//! on the original source, plus VM on formatted output when recovery succeeds.

use leekscript::document::LeekDoc;
use leekscript::format::{FormatOptions, format_leek_doc};
use leekscript::parse::{parse_doc, parse_doc_with_recovery, parse_signature_doc_with_recovery};
use leekscript::scope::run_semantic_analysis;
use leekscript::vm::{Vm, compile_chunk_v4};
use leekscript_fuzz::{bytes_to_string, language_options_for_fuzz, touch_u64};
use libfuzzer_sys::fuzz_target;

const MAX_VM_OPS: u64 = 200_000;

fn run_vm(src: &str) {
    let Ok(chunk) = compile_chunk_v4(src) else {
        return;
    };
    let Ok(mut vm) = Vm::from_compiled_chunk(chunk) else {
        return;
    };
    vm.max_operations = Some(MAX_VM_OPS);
    let _ = vm.run();
    touch_u64(vm.operations);
}

fuzz_target!(|data: &[u8]| {
    let src = bytes_to_string(data);
    let seed = data.first().copied().unwrap_or(0);
    let opts = language_options_for_fuzz(seed, &src);

    // Strict parse (distinct from recovery).
    if let Ok(doc) = parse_doc(&src, opts) {
        touch_u64(doc.source().len() as u64);
    }

    // Signature / stub documents.
    if let Ok(pw) = parse_signature_doc_with_recovery(&src, opts) {
        touch_u64(pw.errors.len() as u64);
        touch_u64(pw.doc.source().len() as u64);
    }

    // Recovery parse → semantics → format (same path as `LeekDoc` / IDE).
    let mut formatted_out: Option<String> = None;
    if let Ok(pw) = parse_doc_with_recovery(&src, opts) {
        touch_u64(pw.errors.len() as u64);
        let doc = LeekDoc::from_parsed(&pw.doc);

        let analysis = run_semantic_analysis(doc.root_syntax(), opts.version);
        touch_u64(analysis.diagnostics.len() as u64);

        let fmt_opts = FormatOptions::default();
        let formatted = format_leek_doc(&doc, &fmt_opts);
        touch_u64(formatted.len() as u64);

        let opts_fmt = language_options_for_fuzz(seed, &formatted);
        if let Ok(doc_fmt) = LeekDoc::parse(&formatted, opts_fmt) {
            let analysis_fmt = run_semantic_analysis(doc_fmt.root_syntax(), opts_fmt.version);
            touch_u64(analysis_fmt.diagnostics.len() as u64);
            let formatted_again = format_leek_doc(&doc_fmt, &fmt_opts);
            touch_u64(formatted_again.len() as u64);
        }

        formatted_out = Some(formatted);
    }

    // VM: always try the raw input; also try formatter output when recovery succeeded.
    run_vm(&src);
    if let Some(fmt) = formatted_out.as_deref() {
        run_vm(fmt);
    }
});
