//! CLI for parsing, formatting, and merging LeekScript sources (`leekscript` binary).

mod args;
mod check;
mod format_cmd;
mod merge_cmd;
mod report;
mod tree;

use std::process::ExitCode;

use clap::Parser;
use leekscript::format::FormatOptions;

use crate::args::{language_options, Cli, Command};
use crate::format_cmd::{build_format_options, cmd_format, load_format_config, FormatDest};

fn main() -> ExitCode {
    report::install_hook();
    let cli = Cli::parse();
    let lang = language_options(&cli);

    match cli.command {
        Command::Check {
            parse_only,
            root,
            signature_files,
            files,
        } => check::cmd_check(
            lang,
            parse_only,
            root.as_deref(),
            &signature_files,
            &files,
        ),
        Command::Format {
            merge_includes,
            root: format_root,
            stdout,
            out,
            config,
            indent_width,
            use_tabs,
            tab_width,
            line_width,
            brace_style,
            blank_lines_between_top_level,
            blank_lines_after_class,
            space_after_keyword_before_paren,
            space_before_function_decl_paren,
            space_inside_parens,
            space_around_assign,
            space_around_binary_ops,
            space_after_comma,
            space_around_type_operators,
            newline_before_else_catch_finally,
            trailing_newline,
            blank_lines_between_block_statements,
            blank_lines_between_class_members,
            max_consecutive_blank_lines_in_block,
            line_ending,
            semicolon_style,
            files,
        } => {
            let base_from_config = match config {
                Some(path) => match load_format_config(&path) {
                    Ok(o) => o,
                    Err(e) => {
                        report::emit(report::format_config(&path, e));
                        return ExitCode::from(2);
                    }
                },
                None => FormatOptions::default(),
            };

            let opts = build_format_options(
                base_from_config,
                indent_width,
                use_tabs,
                tab_width,
                line_width,
                brace_style,
                blank_lines_between_top_level,
                blank_lines_after_class,
                space_after_keyword_before_paren,
                space_before_function_decl_paren,
                space_inside_parens,
                space_around_assign,
                space_around_binary_ops,
                space_after_comma,
                space_around_type_operators,
                newline_before_else_catch_finally,
                trailing_newline,
                blank_lines_between_block_statements,
                blank_lines_between_class_members,
                max_consecutive_blank_lines_in_block,
                line_ending,
                semicolon_style,
            );
            let dest = if stdout {
                FormatDest::Stdout
            } else if let Some(p) = out {
                FormatDest::File(p)
            } else {
                FormatDest::InPlace
            };
            cmd_format(
                lang,
                dest,
                &files,
                &opts,
                merge_includes,
                format_root.as_deref(),
            )
        }
        Command::Merge { root, entry } => merge_cmd::cmd_merge(lang, &root, &entry),
        Command::Tree { trivia, files } => tree::cmd_tree(lang, trivia, &files),
    }
}
