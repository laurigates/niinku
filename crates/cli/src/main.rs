//! niinku: build a Finnish puhekieli/slang dictionary for HeliBoard.
//!
//! Three stages:
//!   - `ingest`   — heavy, per-source corpus crunching → cached freq tables (stub)
//!   - `assemble` — merge cached + live sources, filter, score, emit `.combined`
//!   - `compile`  — invoke `dicttool_aosp.jar makedict` → `.dict`

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use niinku_pipeline::{
    apply_denylist, apply_kirjakieli_filter, apply_min_count, emit_combined_body,
    emit_combined_header, merge, read_token_list, score_table, CombinedHeader, Count, FreqTable,
};
use niinku_sources::{
    opensubtitles::OpenSubtitles, urbaani::UrbaaniSanakirja, voikko::VoikkoLexicon, Source,
};

fn usage() {
    eprintln!(
        "Usage:
  niinku assemble [--data-dir DIR] [--output PATH] [--min-count N]
                  [--freq-min N] [--freq-max N]
                  [--no-opensubtitles] [--no-urbaani]
                  [--no-voikko] [--voikko-dict-path PATH]
                  [--dict-type STR] [--dict-locale STR] [--locale STR]
                  [--description STR] [--version N]
  niinku compile  --combined PATH --output PATH [--jar PATH]
  niinku ingest <source>      (not yet implemented)
"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("assemble") => run_assemble(&args[1..]),
        Some("compile") => run_compile(&args[1..]),
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
    use_voikko: bool,
    voikko_dict_path: Option<String>,
    dict_type: String,
    dict_locale: String,
    locale: String,
    description: String,
    version: u32,
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
            use_voikko: true,
            voikko_dict_path: None,
            dict_type: "puhekieli".into(),
            dict_locale: "fi".into(),
            locale: "fi_FI".into(),
            description: "niinku Finnish puhekieli + slang".into(),
            version: 1,
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
            "--no-voikko" => opts.use_voikko = false,
            "--voikko-dict-path" => {
                opts.voikko_dict_path = Some(val(&mut i, "--voikko-dict-path")?)
            }
            "--dict-type" => opts.dict_type = val(&mut i, "--dict-type")?,
            "--dict-locale" => opts.dict_locale = val(&mut i, "--dict-locale")?,
            "--locale" => opts.locale = val(&mut i, "--locale")?,
            "--description" => opts.description = val(&mut i, "--description")?,
            "--version" => {
                opts.version = val(&mut i, "--version")?
                    .parse()
                    .context("--version: not a u32")?
            }
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
    let allowlist = read_optional_token_list(&opts.data_dir.join("allowlist.txt"))?;
    eprintln!("allowlist: {} entries", allowlist.len());

    let merged = merge(tables);
    eprintln!("merged: {} tokens", merged.len());
    let after_deny = apply_denylist(merged, &denylist);
    let after_min = apply_min_count(after_deny, opts.min_count);
    eprintln!(
        "after denylist + min-count>={}: {} tokens",
        opts.min_count,
        after_min.len()
    );

    let filtered = if opts.use_voikko {
        let lex = VoikkoLexicon::with_path(opts.voikko_dict_path.as_deref())
            .context("initialising Voikko (pass --no-voikko to skip)")?;
        let before = after_min.len();
        let out = apply_kirjakieli_filter(after_min, &lex, &allowlist);
        eprintln!(
            "after voikko kirjakieli filter: {} tokens (dropped {})",
            out.len(),
            before - out.len()
        );
        out
    } else {
        eprintln!("skipping voikko filter (--no-voikko)");
        after_min
    };

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
            let header = CombinedHeader {
                dict_type: opts.dict_type.clone(),
                dict_locale: opts.dict_locale.clone(),
                locale: opts.locale.clone(),
                description: opts.description.clone(),
                date: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                version: opts.version,
            };
            emit_combined_header(&header, &mut w)?;
            emit_combined_body(&entries, &mut w)?;
            w.flush()?;
            eprintln!(
                "wrote header + {} entries to {}",
                entries.len(),
                path.display()
            );
        }
        None => {
            // Body-only on stdout for piping/grep convenience.
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

#[derive(Debug)]
struct CompileOpts {
    combined: PathBuf,
    output: PathBuf,
    jar: PathBuf,
}

fn parse_compile_opts(args: &[String]) -> Result<CompileOpts> {
    let mut combined: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut jar = PathBuf::from("tools/dicttool_aosp.jar");
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
            "--combined" | "-c" => combined = Some(PathBuf::from(val(&mut i, "--combined")?)),
            "--output" | "-o" => output = Some(PathBuf::from(val(&mut i, "--output")?)),
            "--jar" => jar = PathBuf::from(val(&mut i, "--jar")?),
            other => return Err(anyhow!("unknown flag: {other}")),
        }
        i += 1;
    }
    Ok(CompileOpts {
        combined: combined.ok_or_else(|| anyhow!("--combined is required"))?,
        output: output.ok_or_else(|| anyhow!("--output is required"))?,
        jar,
    })
}

fn run_compile(args: &[String]) -> Result<()> {
    let opts = parse_compile_opts(args)?;
    if !opts.jar.exists() {
        return Err(anyhow!(
            "dicttool jar not found at {} — run `just download-jar`",
            opts.jar.display()
        ));
    }
    if !opts.combined.exists() {
        return Err(anyhow!(
            "input .combined not found at {}",
            opts.combined.display()
        ));
    }
    if let Some(parent) = opts.output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    eprintln!(
        "java -jar {} makedict -s {} -d {}",
        opts.jar.display(),
        opts.combined.display(),
        opts.output.display()
    );
    let status = Command::new("java")
        .arg("-jar")
        .arg(&opts.jar)
        .arg("makedict")
        .arg("-s")
        .arg(&opts.combined)
        .arg("-d")
        .arg(&opts.output)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("spawning java — is the JDK installed and on PATH?")?;
    if !status.success() {
        return Err(anyhow!(
            "dicttool exited with status {} — see output above",
            status
        ));
    }
    eprintln!("wrote {}", opts.output.display());
    Ok(())
}
