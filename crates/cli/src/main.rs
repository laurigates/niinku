//! niinku: build a Finnish puhekieli/slang dictionary for HeliBoard.
//!
//! Two stages:
//!   - `ingest`   — heavy, per-source corpus crunching → cached freq tables
//!   - `assemble` — merge cached + live sources, filter, score, emit `.combined`

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use niinku_pipeline::{
    apply_denylist, apply_min_count, emit_combined_body, merge, read_token_list, score_table,
    Count, FreqTable,
};
use niinku_sources::{opensubtitles::OpenSubtitles, urbaani::UrbaaniSanakirja, Source};

fn usage() {
    eprintln!(
        "Usage:
  niinku assemble [--data-dir DIR] [--output PATH] [--min-count N]
                  [--freq-min N] [--freq-max N]
                  [--no-opensubtitles] [--no-urbaani]
  niinku ingest <source>      (not yet implemented)
"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("assemble") => run_assemble(&args[1..]),
        Some("ingest") => {
            eprintln!("niinku ingest: not yet implemented");
            return ExitCode::from(2);
        }
        Some("-h") | Some("--help") | None => {
            usage();
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("niinku: unknown subcommand: {other}");
            usage();
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug)]
struct AssembleOpts {
    data_dir: PathBuf,
    output: Option<PathBuf>,
    min_count: Count,
    freq_min: u8,
    freq_max: u8,
    use_opensubtitles: bool,
    use_urbaani: bool,
}

impl Default for AssembleOpts {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
            output: None,
            min_count: 5,
            freq_min: 100,
            freq_max: 220,
            use_opensubtitles: true,
            use_urbaani: true,
        }
    }
}

fn parse_assemble_opts(args: &[String]) -> Result<AssembleOpts> {
    let mut opts = AssembleOpts::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let val = |i: &mut usize, flag: &str| -> Result<String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| anyhow!("{flag}: missing value"))
        };
        match a.as_str() {
            "--data-dir" => opts.data_dir = PathBuf::from(val(&mut i, "--data-dir")?),
            "--output" | "-o" => opts.output = Some(PathBuf::from(val(&mut i, "--output")?)),
            "--min-count" => {
                opts.min_count = val(&mut i, "--min-count")?
                    .parse()
                    .context("--min-count: not a u64")?
            }
            "--freq-min" => {
                opts.freq_min = val(&mut i, "--freq-min")?
                    .parse()
                    .context("--freq-min: not a u8")?
            }
            "--freq-max" => {
                opts.freq_max = val(&mut i, "--freq-max")?
                    .parse()
                    .context("--freq-max: not a u8")?
            }
            "--no-opensubtitles" => opts.use_opensubtitles = false,
            "--no-urbaani" => opts.use_urbaani = false,
            other => return Err(anyhow!("unknown flag: {other}")),
        }
        i += 1;
    }
    if opts.freq_min > opts.freq_max {
        return Err(anyhow!(
            "--freq-min ({}) must be <= --freq-max ({})",
            opts.freq_min,
            opts.freq_max
        ));
    }
    Ok(opts)
}

fn run_assemble(args: &[String]) -> Result<()> {
    let opts = parse_assemble_opts(args)?;
    let cached = opts.data_dir.join("cached");

    let mut tables: Vec<FreqTable> = Vec::new();
    if opts.use_opensubtitles {
        let path = cached.join("opensubtitles-fi.txt");
        eprintln!("loading opensubtitles-fi from {}", path.display());
        let src = OpenSubtitles::new(&path);
        let t = src
            .fetch()
            .with_context(|| format!("source '{}' failed", src.name()))?;
        eprintln!("  {} tokens", t.len());
        tables.push(t);
    }
    if opts.use_urbaani {
        let path = cached.join("urbaani.txt");
        if path.exists() {
            eprintln!("loading urbaani from {}", path.display());
            let src = UrbaaniSanakirja::new(&path);
            let t = src
                .fetch()
                .with_context(|| format!("source '{}' failed", src.name()))?;
            eprintln!("  {} headwords", t.len());
            tables.push(t);
        } else {
            eprintln!(
                "skipping urbaani: {} not present (use --no-urbaani to silence)",
                path.display()
            );
        }
    }

    if tables.is_empty() {
        return Err(anyhow!("no sources enabled"));
    }

    let denylist = read_optional_token_list(&opts.data_dir.join("denylist.txt"))?;
    eprintln!("denylist: {} entries", denylist.len());

    let merged = merge(tables);
    eprintln!("merged: {} tokens", merged.len());
    let after_deny = apply_denylist(merged, &denylist);
    let filtered = apply_min_count(after_deny, opts.min_count);
    eprintln!(
        "after denylist + min-count>={}: {} tokens",
        opts.min_count,
        filtered.len()
    );

    let entries = score_table(&filtered, opts.freq_min, opts.freq_max);

    match &opts.output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            let mut w = BufWriter::new(
                File::create(path).with_context(|| format!("creating {}", path.display()))?,
            );
            emit_combined_body(&entries, &mut w)?;
            w.flush()?;
            eprintln!("wrote {} entries to {}", entries.len(), path.display());
        }
        None => {
            let mut stdout = std::io::stdout().lock();
            emit_combined_body(&entries, &mut stdout)?;
        }
    }
    Ok(())
}

fn read_optional_token_list(path: &Path) -> Result<HashSet<String>> {
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    Ok(read_token_list(BufReader::new(f))?)
}
