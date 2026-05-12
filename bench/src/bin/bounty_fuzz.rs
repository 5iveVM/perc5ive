//! `cargo run --bin bounty_fuzz -- --target <class> --probes <n>`
//!
//! Run a target's probes and stream `ProbeOutcome` records as JSONL to
//! `bench/fuzz_results/<timestamp>.jsonl`. Each line is one probe's
//! outcome (probe id + zero or more divergences). The `--target` value is
//! one of `sanity`, `t1_funding`, `t2_conservation`, `t3_margin`,
//! `t4_riskbuffer` (aliases: `t1`–`t4`).
//!
//! Session 2 status: sanity target produces real comparisons (1000 runs
//! must emit zero divergences — that's the harness plumbing check); T1–T4
//! are stubs that emit clean outcomes. Session 3 fills in the real
//! adversarial probe payloads.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use perc5ive_bench::bounty_fuzz::{BpfRunner, Target, run_target};

#[derive(Debug, Parser)]
#[command(
    name = "bounty_fuzz",
    about = "Three-way differential harness for the percolator bounty hunt"
)]
struct Args {
    /// Target bug class to fuzz.
    #[arg(long, value_parser = parse_target)]
    target: Target,

    /// Number of probes to run.
    #[arg(long, default_value_t = 1000)]
    probes: u64,

    /// PRNG seed for reproducible probe inputs.
    #[arg(long, default_value_t = 0xC0FFEE_u64)]
    seed: u64,

    /// Output directory. Defaults to `bench/fuzz_results/` (or
    /// `fuzz_results/` if cwd is bench/).
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// If set, also try to construct a BpfRunner to verify the BPF leg
    /// loads. Off by default so `cargo run` in environments without a
    /// built `.so` doesn't fail.
    #[arg(long, default_value_t = false)]
    require_bpf: bool,
}

fn parse_target(s: &str) -> Result<Target, String> {
    s.parse()
}

fn default_out_dir() -> PathBuf {
    let candidates = [PathBuf::from("bench/fuzz_results"), PathBuf::from("fuzz_results")];
    candidates
        .into_iter()
        .find(|p| {
            p.exists()
                || p.parent()
                    .map(|parent| parent.exists())
                    .unwrap_or(false)
        })
        .unwrap_or_else(|| PathBuf::from("bench/fuzz_results"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let out_dir = args.out_dir.unwrap_or_else(default_out_dir);
    std::fs::create_dir_all(&out_dir)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out_path = out_dir.join(format!("{ts}_{}.jsonl", args.target.as_str()));

    // BPF leg: only construct if requested. We surface the load error
    // loudly so harness failures don't masquerade as bug-class candidates.
    if args.require_bpf {
        match BpfRunner::from_default() {
            Ok(runner) => {
                eprintln!(
                    "bpf_runner: loaded {} ({} bytes)",
                    runner.so_path().display(),
                    std::fs::metadata(runner.so_path())
                        .map(|m| m.len())
                        .unwrap_or(0)
                );
            }
            Err(e) => {
                eprintln!("bpf_runner: load failed — {e}");
                std::process::exit(2);
            }
        }
    }

    let outcomes = run_target(args.target, args.probes, args.seed);
    let divergence_count: usize = outcomes.iter().map(|o| o.divergences.len()).sum();
    let dirty = outcomes.iter().filter(|o| !o.is_clean()).count();

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&out_path)?;
    let mut writer = BufWriter::new(file);
    for o in &outcomes {
        let line = serde_json::to_string(o)?;
        writeln!(writer, "{line}")?;
    }
    writer.flush()?;

    eprintln!(
        "bounty_fuzz: target={} probes={} divergences={} dirty_probes={} → {}",
        args.target.as_str(),
        args.probes,
        divergence_count,
        dirty,
        out_path.display()
    );
    Ok(())
}
