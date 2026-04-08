use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use miette::{Context, IntoDiagnostic};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    JsonPretty,
    FightReport,
    ActionsRaw,
    Snapshot,
}

#[derive(Debug, Parser)]
#[command(name = "leekwars-generator", version)]
struct Args {
    /// Analyze an AI file (LeekScript diagnostics as JSON).
    #[arg(long)]
    analyze: bool,

    /// After running a scenario, use a human-readable fight timeline (like the web report) instead of raw JSON.
    /// Printed to stdout; with `--out`, the same text is written to the file and still shown on stdout.
    #[arg(long)]
    fight_report: bool,

    /// Output format (preferred over legacy `--fight-report`).
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Pretty-print JSON output (alias for `--format json-pretty`).
    #[arg(long)]
    pretty: bool,

    #[arg(long)]
    verbose: bool,

    #[arg(long)]
    out: Option<PathBuf>,

    /// Game data directory for names (expects `weapons.json`; `chips.json` optional).
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// After running a scenario, output a reconstructed snapshot after replaying actions up to this index.
    #[arg(long)]
    snapshot_at: Option<usize>,

    /// Convenience alias for `--format actions-raw`.
    #[arg(long)]
    timeline_only: bool,

    /// Remove `logs` from the JSON output (useful for stable golden tests).
    #[arg(long)]
    no_logs: bool,

    /// Collect per-turn VM run info for a single entity id into `logs.ai_run[entity]`.
    #[arg(long)]
    trace_entity: Option<i64>,

    /// Persistent registers JSON file (all entities in one file).
    #[arg(long)]
    registers_file: Option<PathBuf>,

    /// Persistent registers directory (one `<id>.json` per entity).
    #[arg(long)]
    registers_dir: Option<PathBuf>,

    /// Reset registers storage before running (delete the file/dir).
    #[arg(long)]
    reset_registers: bool,

    /// Run a batch job from TOML or JSON (`batch-configs/*.toml`). Prints a `BatchResult` JSON object (summary + all outcomes).
    #[cfg(feature = "batch")]
    #[arg(long, conflicts_with_all = ["analyze", "fight_report", "format", "pretty", "snapshot_at", "timeline_only", "no_logs"])]
    batch: Option<PathBuf>,

    /// With `--batch`: write the human-readable batch report to this path (UTF-8 text).
    #[cfg(feature = "batch")]
    #[arg(long, requires = "batch")]
    batch_text_out: Option<PathBuf>,

    /// With `--batch` and `--out`: do not print the human-readable report to stdout (JSON still written to `--out`).
    /// Ignored when `--batch-report` is set (that flag always prints the report to stdout).
    #[cfg(feature = "batch")]
    #[arg(long, requires = "batch")]
    batch_no_human_stdout: bool,

    /// With `--batch`: always print the human-readable batch report to stdout.
    /// With `--out` and no `--batch-report`, the report is printed by default unless `--batch-no-human-stdout` is set.
    /// With `--batch-report` and no `--out`, JSON is not printed to stdout (human report only).
    #[cfg(feature = "batch")]
    #[arg(long, requires = "batch")]
    batch_report: bool,

    /// With `--batch`: do not print run progress to stderr (default is progress on stderr so stdout stays clean).
    #[cfg(feature = "batch")]
    #[arg(long, requires = "batch")]
    no_batch_progress: bool,

    /// Scenario JSON path (default) or AI path (with `--analyze`).
    #[cfg_attr(feature = "batch", doc = " Or omit when using `--batch`.")]
    file: Option<PathBuf>,
}

fn emit_scenario_output(args: &Args, payload: &str) -> miette::Result<()> {
    let payload: Cow<'_, str> = Cow::Borrowed(payload);

    if let Some(out) = &args.out {
        fs::write(out, payload.as_bytes())
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write report to `{}`", out.display()))?;
    }
    // With `--fight-report` and `--out`, still show the report on stdout; plain `--out` stays file-only.
    let show_stdout = args.out.is_none()
        || matches!(
            effective_format(args),
            OutputFormat::FightReport
        )
        || args.fight_report;
    if show_stdout {
        println!("{payload}");
    }
    Ok(())
}

fn effective_format(args: &Args) -> OutputFormat {
    if let Some(f) = args.format {
        return f;
    }
    if args.fight_report {
        return OutputFormat::FightReport;
    }
    if args.pretty {
        return OutputFormat::JsonPretty;
    }
    if args.timeline_only {
        return OutputFormat::ActionsRaw;
    }
    if args.snapshot_at.is_some() {
        return OutputFormat::Snapshot;
    }
    OutputFormat::Json
}

fn main() -> miette::Result<()> {
    let args = Args::parse();

    #[cfg(feature = "batch")]
    if let Some(batch_path) = &args.batch {
        let generator = leekwars_generator_rs::Generator {
            verbose: args.verbose,
            signature_files: leekwars_generator_rs::Generator::new().signature_files,
            register_manager: None,
            trace_entity: None,
        };
        let runner = leekwars_generator_rs::BatchRunner {
            generator,
            show_progress: !args.no_batch_progress,
        };
        let job = leekwars_generator_rs::BatchRunner::load_job_from_file(batch_path)?;
        let result = runner.run(&job)?;
        let payload = serde_json::to_string_pretty(&result).into_diagnostic()?;
        let human = leekwars_generator_rs::format_batch_human(&job, &result);

        if args.batch_report && args.out.is_none() {
            println!("{human}");
            if let Some(p) = &args.batch_text_out {
                fs::write(p, human.as_bytes())
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to write batch report to `{}`", p.display()))?;
            }
            return Ok(());
        }

        if let Some(out) = &args.out {
            fs::write(out, payload.as_bytes())
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write batch result to `{}`", out.display()))?;
            let human_stdout = args.batch_report || !args.batch_no_human_stdout;
            if human_stdout {
                println!("{human}");
            }
        } else {
            println!("{payload}");
        }

        if let Some(p) = &args.batch_text_out {
            fs::write(p, human.as_bytes())
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write batch report to `{}`", p.display()))?;
        }
        return Ok(());
    }

    let Some(file) = args.file.clone() else {
        #[cfg(feature = "batch")]
        return Err(miette::miette!(
            "missing scenario or AI path: pass `file` or use `--batch <job.toml>`"
        ));
        #[cfg(not(feature = "batch"))]
        return Err(miette::miette!("missing scenario or AI path: pass `file`"));
    };

    if args.analyze && args.fight_report {
        return Err(miette::miette!(
            "`--analyze` is for AI files; `--fight-report` applies to scenario runs. Use only one."
        ));
    }

    if args.analyze {
        let src = fs::read_to_string(&file)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read AI file `{}`", file.display()))?;

        let diags = leekwars_generator_rs::analyze_ai_source_with_path(&src, Some(&file))
            .map_err(|e| miette::miette!("{e:?}"))
            .wrap_err("failed to parse AI")?;

        let report = serde_json::to_string(&diags).into_diagnostic()?;
        if let Some(out) = &args.out {
            fs::write(out, report.as_bytes())
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write report to `{}`", out.display()))?;
        } else {
            println!("{report}");
        }
        return Ok(());
    }

    let generator = leekwars_generator_rs::Generator {
        verbose: args.verbose,
        signature_files: leekwars_generator_rs::Generator::new().signature_files,
        register_manager: {
            if args.registers_file.is_some() && args.registers_dir.is_some() {
                return Err(miette::miette!("use only one of `--registers-file` or `--registers-dir`"));
            }
            if let Some(p) = &args.registers_file {
                if args.reset_registers {
                    let _ = std::fs::remove_file(p);
                }
                Some(std::rc::Rc::new(leekwars_generator_rs::FileRegisterManager::new(p)) as leekwars_generator_rs::RegisterManagerRc)
            } else if let Some(d) = &args.registers_dir {
                let mgr = leekwars_generator_rs::DirRegisterManager::new(d);
                if args.reset_registers {
                    mgr.reset();
                }
                Some(std::rc::Rc::new(mgr) as leekwars_generator_rs::RegisterManagerRc)
            } else {
                None
            }
        },
        trace_entity: args.trace_entity,
    };
    let outcome = generator.run_scenario_from_file(&file)?;
    let format = effective_format(&args);
    let payload = match format {
        OutputFormat::Json | OutputFormat::JsonPretty => {
            if args.no_logs {
                let mut v = serde_json::to_value(&outcome).into_diagnostic()?;
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("logs");
                }
                if matches!(format, OutputFormat::JsonPretty) {
                    serde_json::to_string_pretty(&v).into_diagnostic()?
                } else {
                    serde_json::to_string(&v).into_diagnostic()?
                }
            } else if matches!(format, OutputFormat::JsonPretty) {
                serde_json::to_string_pretty(&outcome).into_diagnostic()?
            } else {
                serde_json::to_string(&outcome).into_diagnostic()?
            }
        }
        OutputFormat::ActionsRaw => {
            let actions = outcome.fight.get("actions").cloned().unwrap_or(serde_json::Value::Null);
            serde_json::to_string(&actions).into_diagnostic()?
        }
        OutputFormat::Snapshot => {
            let Some(i) = args.snapshot_at else {
                return Err(miette::miette!("`--format snapshot` requires `--snapshot-at <index>`"));
            };
            let snap = outcome.snapshot_at(i)?;
            serde_json::to_string(&snap).into_diagnostic()?
        }
        OutputFormat::FightReport => {
            let v = serde_json::to_value(&outcome).into_diagnostic()?;
            if let Some(dir) = &args.data_dir {
                let game = leekwars_generator_rs::GameNames::load_from_data_dir(dir);
                leekwars_generator_rs::format_outcome_human_with_game(&v, &game)
            } else {
                leekwars_generator_rs::format_outcome_human_for_path(&v, &file)
            }
        }
    };
    emit_scenario_output(&args, &payload)?;

    Ok(())
}
