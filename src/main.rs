use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;

use fa10::grow::{GrowOptions, Target};
use fa10::progress::NoProgress;
use fa10::restore::RestoreOptions;
use fa10::{grow, info, restore, safety, to_hex};

mod cli;
use cli::{Cli, Commands, GrowArgs, InfoArgs, RestoreArgs, ThemedArgs};

const BANNER: &str = concat!(
    "fa10 v",
    env!("CARGO_PKG_VERSION"),
    " - grow a file into a larger, fully-reversible test file.\n",
    "It appends recognizable padding; `fa10 restore` recovers the exact original.\n",
    "Local filesystem only: no network, no persistence, no self-modification.\n",
);

/// An `indicatif`-backed progress sink (only when the `progress` feature is on).
#[cfg(feature = "progress")]
struct BarProgress {
    bar: indicatif::ProgressBar,
}

#[cfg(feature = "progress")]
impl BarProgress {
    fn new(label: &str) -> Self {
        let bar = indicatif::ProgressBar::new(0);
        bar.set_style(
            indicatif::ProgressStyle::with_template(
                "{msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec})",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        bar.set_message(label.to_string());
        BarProgress { bar }
    }
}

#[cfg(feature = "progress")]
impl fa10::progress::Progress for BarProgress {
    fn set_total(&self, total: u64) {
        self.bar.set_length(total);
    }
    fn add(&self, delta: u64) {
        self.bar.inc(delta);
    }
    fn finish(&self) {
        self.bar.finish_and_clear();
    }
}

fn main() {
    let cli = Cli::parse_from(inject_default_subcommand(std::env::args_os().collect()));
    if let Err(err) = run(&cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

/// Make `grow` the implicit default: `fa10 report.csv` behaves like
/// `fa10 grow report.csv`. If the first non-global token is not a known
/// subcommand (and not a help/version request), insert `grow` before it.
/// Leading global flags (`-q`/`-v` and their combinations) are skipped so
/// `fa10 -q report.csv` still resolves to grow.
fn inject_default_subcommand(mut args: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let mut i = 1; // args[0] is the program name
    while i < args.len() {
        let tok = args[i].to_string_lossy();
        // Skip global flags that may legitimately precede a subcommand.
        let is_global_short_combo = tok.starts_with('-')
            && !tok.starts_with("--")
            && tok.len() > 1
            && tok.chars().skip(1).all(|c| c == 'q' || c == 'v');
        if is_global_short_combo || tok == "--quiet" || tok == "--verbose" {
            i += 1;
            continue;
        }
        // Let clap handle help/version directly.
        if tok == "-h" || tok == "--help" || tok == "-V" || tok == "--version" {
            break;
        }
        // A known subcommand: nothing to inject.
        if cli::SUBCOMMANDS.contains(&tok.as_ref()) {
            break;
        }
        // Otherwise this is the implicit grow path.
        args.insert(i, std::ffi::OsString::from("grow"));
        break;
    }
    args
}

fn run(cli: &Cli) -> Result<()> {
    if !cli.quiet {
        eprint!("{BANNER}");
    }
    match &cli.command {
        Commands::Grow(args) => run_grow(args, cli),
        Commands::Cake(args) => run_themed(args, 2.0, cli),
        Commands::Feast(args) => run_themed(args, 5.0, cli),
        Commands::Buffet(args) => run_themed(args, 10.0, cli),
        Commands::Restore(args) => run_restore(args, cli),
        Commands::Info(args) => run_info(args, cli),
    }
}

/// Run `f` with a progress bar (label shown) unless `quiet`. With the `progress`
/// feature disabled the bar is compiled out and `f` always gets `NoProgress`.
#[cfg(feature = "progress")]
fn with_progress<T>(
    quiet: bool,
    label: &str,
    f: impl FnOnce(&dyn fa10::progress::Progress) -> T,
) -> T {
    if quiet {
        f(&NoProgress)
    } else {
        let bar = BarProgress::new(label);
        f(&bar)
    }
}

#[cfg(not(feature = "progress"))]
fn with_progress<T>(
    _quiet: bool,
    _label: &str,
    f: impl FnOnce(&dyn fa10::progress::Progress) -> T,
) -> T {
    f(&NoProgress)
}

fn target_from(multiplier: Option<f64>, size: &Option<String>) -> Result<Target> {
    if let Some(s) = size {
        let bytes = fa10::parse_size(s)?;
        Ok(Target::Size(bytes))
    } else {
        Ok(Target::Multiplier(multiplier.unwrap_or(2.0)))
    }
}

fn run_grow(args: &GrowArgs, cli: &Cli) -> Result<()> {
    let target = target_from(args.multiplier, &args.size)?;
    grow_files(
        &args.files,
        target,
        &args.output,
        &args.pattern,
        args.in_place,
        args.confirm,
        args.verify,
        args.batch,
        cli,
    )
}

fn run_themed(args: &ThemedArgs, multiplier: f64, cli: &Cli) -> Result<()> {
    grow_files(
        &args.files,
        Target::Multiplier(multiplier),
        &args.output,
        &args.pattern,
        args.in_place,
        args.confirm,
        args.verify,
        args.batch,
        cli,
    )
}

#[allow(clippy::too_many_arguments)]
fn grow_files(
    files: &[PathBuf],
    target: Target,
    output: &Option<PathBuf>,
    pattern: &Option<String>,
    in_place: bool,
    confirm: bool,
    verify: bool,
    batch: bool,
    cli: &Cli,
) -> Result<()> {
    safety::check_batch_limit(files.len(), batch)?;
    if files.len() > 1 && output.is_some() {
        bail!("--output can only be used with a single input file");
    }

    for file in files {
        let mut opts = GrowOptions::new(file.clone(), target.clone());
        opts.output = output.clone();
        opts.in_place = in_place;
        opts.confirm = confirm;
        opts.verify = verify;
        if let Some(p) = pattern {
            opts.pattern = p.clone();
        }

        let outcome = with_progress(cli.quiet, &format!("growing {}", file.display()), |p| {
            grow::grow(&opts, p)
        })
        .with_context(|| format!("failed to grow {}", file.display()))?;

        if !cli.quiet {
            if outcome.clamped {
                eprintln!(
                    "note: requested size was below the minimum; grew to the smallest reversible size."
                );
            }
            println!(
                "grew {} -> {} ({} -> {}, {} padding){}",
                file.display(),
                outcome.output_path.display(),
                human(outcome.original_size),
                human(outcome.output_size),
                human(outcome.padding_size),
                if verify { ", verified" } else { "" },
            );
            if cli.verbose {
                println!("  sha256: {}", to_hex(&outcome.sha256));
            }
        }
    }
    Ok(())
}

fn run_restore(args: &RestoreArgs, cli: &Cli) -> Result<()> {
    safety::check_batch_limit(args.files.len(), args.batch)?;
    if args.files.len() > 1 && args.output.is_some() {
        bail!("--output can only be used with a single input file");
    }

    for file in &args.files {
        let mut opts = RestoreOptions::new(file.clone());
        opts.output = args.output.clone();
        opts.verify = !args.no_verify;
        opts.force = args.force;

        let outcome = with_progress(cli.quiet, &format!("restoring {}", file.display()), |p| {
            restore::restore(&opts, p)
        })
        .with_context(|| format!("failed to restore {}", file.display()))?;

        if !cli.quiet {
            println!(
                "restored {} -> {} ({}){}",
                file.display(),
                outcome.output_path.display(),
                human(outcome.original_size),
                if outcome.verified {
                    ", SHA-256 verified"
                } else {
                    ""
                },
            );
        }
    }
    Ok(())
}

fn run_info(args: &InfoArgs, _cli: &Cli) -> Result<()> {
    let info = info::info(&args.file)?;
    println!("file:              {}", info.path.display());
    println!("original filename: {}", info.original_filename);
    println!(
        "original size:     {} ({} bytes)",
        human(info.original_size),
        info.original_size
    );
    println!(
        "total size:        {} ({} bytes)",
        human(info.total_size),
        info.total_size
    );
    println!(
        "padding size:      {} ({} bytes)",
        human(info.padding_size),
        info.padding_size
    );
    println!("footer size:       {} bytes", info.footer_size);
    println!("multiplier:        {:.2}x", info.multiplier);
    println!("original sha256:   {}", info.sha256_hex());
    Ok(())
}

// --- small formatting helpers ---

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}
