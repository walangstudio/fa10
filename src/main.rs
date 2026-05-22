use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

use fa10::grow::{GrowOptions, Target};
use fa10::progress::{NoProgress, Progress};
use fa10::restore::RestoreOptions;
use fa10::{grow, info, restore, safety};

mod cli;
use cli::{Cli, Commands, GrowArgs, InfoArgs, RestoreArgs, ThemedArgs};

const BANNER: &str = concat!(
    "fa10 v",
    env!("CARGO_PKG_VERSION"),
    " - grow a file into a larger, fully-reversible test file.\n",
    "It appends recognizable padding; `fa10 restore` recovers the exact original.\n",
    "Local filesystem only: no network, no persistence, no self-modification.\n",
);

/// An `indicatif`-backed progress sink.
struct BarProgress {
    bar: ProgressBar,
}

impl BarProgress {
    fn new(label: &str) -> Self {
        let bar = ProgressBar::new(0);
        bar.set_style(
            ProgressStyle::with_template(
                "{msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec})",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        bar.set_message(label.to_string());
        BarProgress { bar }
    }
}

impl Progress for BarProgress {
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
    let cli = Cli::parse();
    if let Err(err) = run(&cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
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

        let outcome = if cli.quiet {
            grow::grow(&opts, &NoProgress)
        } else {
            let bar = BarProgress::new(&format!("growing {}", file.display()));
            grow::grow(&opts, &bar)
        }
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
                println!("  sha256: {}", hex(&outcome.sha256));
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

        let outcome = if cli.quiet {
            restore::restore(&opts, &NoProgress)
        } else {
            let bar = BarProgress::new(&format!("restoring {}", file.display()));
            restore::restore(&opts, &bar)
        }
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

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
