//! `leekscript merge` — emit merged source for an entry file.

use std::path::Path;
use std::process::ExitCode;

use leekscript::{LanguageOptions, prepare_merged_check_unit};

use crate::report;

pub(crate) fn cmd_merge(lang: LanguageOptions, root: &Path, entry: &Path) -> ExitCode {
    match prepare_merged_check_unit(root, entry, lang, &[], None) {
        Ok(prep) => {
            print!("{}", prep.combined);
            ExitCode::SUCCESS
        }
        Err(e) => {
            report::emit(report::merged_check_prep(
                root,
                entry,
                e,
                report::MergedCheckPrepContext::MergeSubcommand,
            ));
            ExitCode::from(1)
        }
    }
}
