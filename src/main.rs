use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use fa10::grow::{GrowOptions, Target};
use fa10::progress::NoProgress;
use fa10::restore::RestoreOptions;
use fa10::{grow, info, restore, to_hex};

mod cli;
use cli::{Cli, Commands, GrowArgs, InfoArgs, RestoreArgs, ThemedArgs};

const BANNER: &str = concat!(
    "fa10 v",
    env!("CARGO_PKG_VERSION"),
    " - pack files and directories into one larger, fully-reversible archive.\n",
    "Recognizable padding instead of compression; `fa10 restore` rebuilds the tree.\n",
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
    grow_archive(args.files.clone(), target, args, cli)
}

fn run_themed(args: &ThemedArgs, multiplier: f64, cli: &Cli) -> Result<()> {
    let mut opts = GrowOptions::new(args.files.clone(), Target::Multiplier(multiplier));
    opts.output = args.output.clone();
    opts.in_place = args.in_place;
    opts.confirm = args.confirm;
    opts.verify = args.verify;
    opts.batch = args.batch;
    if let Some(p) = &args.pattern {
        opts.pattern = p.clone();
    }
    pack(&opts, cli)
}

fn grow_archive(files: Vec<PathBuf>, target: Target, args: &GrowArgs, cli: &Cli) -> Result<()> {
    let mut opts = GrowOptions::new(files, target);
    opts.output = args.output.clone();
    opts.in_place = args.in_place;
    opts.confirm = args.confirm;
    opts.verify = args.verify;
    opts.batch = args.batch;
    if let Some(p) = &args.pattern {
        opts.pattern = p.clone();
    }
    pack(&opts, cli)
}

fn pack(opts: &GrowOptions, cli: &Cli) -> Result<()> {
    let outcome = with_progress(cli.quiet, "packing", |p| grow::grow(opts, p))
        .with_context(|| "failed to create archive".to_string())?;

    if !cli.quiet {
        if outcome.clamped {
            eprintln!(
                "note: requested size was below the minimum; grew to the smallest reversible size."
            );
        }
        let entries = if outcome.entry_count == 1 {
            "1 entry".to_string()
        } else {
            format!("{} entries", outcome.entry_count)
        };
        println!(
            "packed {} ({}) -> {} ({}, {} padding){}",
            entries,
            human(outcome.payload_size),
            outcome.output_path.display(),
            human(outcome.output_size),
            human(outcome.padding_size),
            if opts.verify { ", verified" } else { "" },
        );
    }
    Ok(())
}

fn run_restore(args: &RestoreArgs, cli: &Cli) -> Result<()> {
    for file in &args.files {
        let mut opts = RestoreOptions::new(file.clone());
        opts.output = args.output.clone();
        opts.verify = !args.no_verify;
        opts.force = args.force;

        let outcome = with_progress(cli.quiet, &format!("extracting {}", file.display()), |p| {
            restore::restore(&opts, p)
        })
        .with_context(|| format!("failed to extract {}", file.display()))?;

        if !cli.quiet {
            let entries = if outcome.entry_count == 1 {
                "1 entry".to_string()
            } else {
                format!("{} entries", outcome.entry_count)
            };
            println!(
                "extracted {} from {} -> {} ({}){}",
                entries,
                file.display(),
                outcome.root.display(),
                human(outcome.payload_size),
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

fn run_info(args: &InfoArgs, cli: &Cli) -> Result<()> {
    let info = info::info(&args.file)?;
    println!("archive:      {}", info.path.display());
    println!(
        "total size:   {} ({} bytes)",
        human(info.total_size),
        info.total_size
    );
    println!(
        "payload size: {} ({} bytes)",
        human(info.payload_size),
        info.payload_size
    );
    println!(
        "padding size: {} ({} bytes)",
        human(info.padding_size),
        info.padding_size
    );
    println!("manifest:     {} bytes", info.manifest_size);
    println!("multiplier:   {:.2}x", info.multiplier);
    println!("entries:      {}", info.entry_count);
    for e in &info.entries {
        match e.kind {
            fa10::EntryKind::EmptyDir => println!("  {:>12}  {}/", "<dir>", e.path),
            fa10::EntryKind::File if cli.verbose => {
                println!("  {:>12}  {}  {}", e.size, e.path, to_hex(&e.sha256))
            }
            fa10::EntryKind::File => println!("  {:>12}  {}", e.size, e.path),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::inject_default_subcommand;
    use std::ffi::OsString;

    /// Run the injector on a borrowed token list and return owned strings.
    fn inject(parts: &[&str]) -> Vec<String> {
        let args: Vec<OsString> = parts.iter().map(OsString::from).collect();
        inject_default_subcommand(args)
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn bare_file_gets_grow() {
        assert_eq!(
            inject(&["fa10", "report.csv"]),
            ["fa10", "grow", "report.csv"]
        );
    }

    #[test]
    fn multiple_files_get_grow_once() {
        assert_eq!(
            inject(&["fa10", "a.bin", "b.bin"]),
            ["fa10", "grow", "a.bin", "b.bin"]
        );
    }

    #[test]
    fn top_level_long_flag_implies_grow() {
        assert_eq!(
            inject(&["fa10", "--multiplier", "5", "f"]),
            ["fa10", "grow", "--multiplier", "5", "f"]
        );
    }

    #[test]
    fn top_level_short_flag_implies_grow() {
        assert_eq!(
            inject(&["fa10", "-m", "5", "f"]),
            ["fa10", "grow", "-m", "5", "f"]
        );
    }

    #[test]
    fn equals_form_flag_implies_grow() {
        assert_eq!(
            inject(&["fa10", "--size=100MB", "f"]),
            ["fa10", "grow", "--size=100MB", "f"]
        );
    }

    #[test]
    fn explicit_grow_is_untouched() {
        assert_eq!(inject(&["fa10", "grow", "f"]), ["fa10", "grow", "f"]);
    }

    #[test]
    fn known_subcommands_and_aliases_untouched() {
        for sub in [
            "restore", "info", "cake", "feast", "buffet", "diet", "slim", "help",
        ] {
            assert_eq!(inject(&["fa10", sub, "x"]), ["fa10", sub, "x"], "sub={sub}");
        }
    }

    #[test]
    fn global_flags_are_skipped_then_grow_injected() {
        assert_eq!(inject(&["fa10", "-q", "f"]), ["fa10", "-q", "grow", "f"]);
        assert_eq!(
            inject(&["fa10", "--quiet", "f"]),
            ["fa10", "--quiet", "grow", "f"]
        );
        assert_eq!(
            inject(&["fa10", "--verbose", "f"]),
            ["fa10", "--verbose", "grow", "f"]
        );
        assert_eq!(inject(&["fa10", "-qv", "f"]), ["fa10", "-qv", "grow", "f"]);
        assert_eq!(inject(&["fa10", "-vq", "f"]), ["fa10", "-vq", "grow", "f"]);
    }

    #[test]
    fn global_flag_before_subcommand_is_untouched() {
        assert_eq!(
            inject(&["fa10", "-q", "cake", "f"]),
            ["fa10", "-q", "cake", "f"]
        );
    }

    #[test]
    fn help_and_version_are_left_for_clap() {
        for flag in ["-h", "--help", "-V", "--version"] {
            assert_eq!(inject(&["fa10", flag]), ["fa10", flag], "flag={flag}");
        }
    }

    #[test]
    fn version_short_flag_is_not_a_verbose_combo() {
        // -V (version) must not be mistaken for a -v (verbose) global combo.
        assert_eq!(inject(&["fa10", "-V"]), ["fa10", "-V"]);
    }

    #[test]
    fn flag_value_that_looks_like_a_subcommand_still_grows() {
        // `restore` here is the value of --pattern, not a subcommand.
        assert_eq!(
            inject(&["fa10", "--pattern", "restore", "f"]),
            ["fa10", "grow", "--pattern", "restore", "f"]
        );
    }

    #[test]
    fn no_arguments_is_untouched() {
        assert_eq!(inject(&["fa10"]), ["fa10"]);
    }

    #[test]
    fn only_global_flags_injects_nothing() {
        // No positional/subcommand to act on; clap will show help.
        assert_eq!(inject(&["fa10", "-q"]), ["fa10", "-q"]);
    }
}
