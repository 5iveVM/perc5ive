//! BPF runner — loads the compiled `percolator-prog` shared object via
//! litesvm and gives the harness a way to construct instructions, invoke
//! them in the SVM, and read the slab account back.
//!
//! Session 2 scope: minimal — open the `.so`, register the program at the
//! canonical `Perco1ator111111111111111111111111111111111` id, and expose
//! `airdrop` / `get_account` so probes can build their own scenarios. The
//! instruction-construction helpers grow with each target as Session 3
//! plugs probes in.

use std::path::{Path, PathBuf};

use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address as Pubkey;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// The canonical percolator-prog program id (declared in
/// `aeyakovenko/percolator-prog/src/percolator.rs` via `declare_id!`).
/// Hard-coded here so the harness doesn't depend on linking percolator-prog
/// in our Cargo graph — the BPF binary already knows its own id at the
/// `declare_id!` site.
pub const PERCOLATOR_PROG_ID: &str = "Perco1ator111111111111111111111111111111111";

/// Default location of the compiled BPF binary, relative to the perc5ive
/// repo root. Overridable via the `PERCOLATOR_PROG_SO` env var.
pub const DEFAULT_SO_PATH: &str =
    "hello_slab/percolator-prog/target/deploy/percolator_prog.so";

#[derive(Debug, thiserror::Error)]
pub enum BpfRunnerError {
    #[error("percolator_prog.so not found at {0}; run `cargo build-sbf` inside hello_slab/percolator-prog first")]
    SoMissing(PathBuf),
    #[error("failed to read percolator_prog.so: {0}")]
    SoRead(#[from] std::io::Error),
    #[error("invalid program id {0}")]
    BadProgramId(String),
    #[error("litesvm add_program failed: {0}")]
    AddProgram(String),
}

/// Thin wrapper around `LiteSVM` with percolator-prog pre-loaded.
///
/// The runner is intentionally state-light: each target/probe constructs
/// its own transaction and runs it via `.svm().send_transaction(...)`. We
/// don't try to cache slab keypairs or remember per-target scenarios at
/// the runner level — that lives in `targets::*`.
pub struct BpfRunner {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    so_path: PathBuf,
}

impl BpfRunner {
    /// Construct a runner loading the `.so` from `so_path` (use
    /// `Self::default_so_path()` if you want the canonical location).
    pub fn new(so_path: impl AsRef<Path>) -> Result<Self, BpfRunnerError> {
        let so_path = so_path.as_ref().to_path_buf();
        if !so_path.exists() {
            return Err(BpfRunnerError::SoMissing(so_path));
        }

        let program_id: Pubkey = PERCOLATOR_PROG_ID
            .parse()
            .map_err(|_| BpfRunnerError::BadProgramId(PERCOLATOR_PROG_ID.to_string()))?;

        let mut svm = LiteSVM::new();
        let program_bytes = std::fs::read(&so_path)?;
        svm.add_program(program_id, &program_bytes)
            .map_err(|e| BpfRunnerError::AddProgram(format!("{e:?}")))?;

        let payer = Keypair::new();
        // Fund the payer generously — probe scenarios stand up slabs +
        // user accounts + matchers and pay rent + fees from this wallet.
        // Keypair::pubkey() returns solana_pubkey::Pubkey while litesvm
        // takes solana_address::Address — same 32-byte payload, different
        // wrapper type in the split-crate world, so bridge via bytes.
        let payer_addr = Pubkey::new_from_array(payer.pubkey().to_bytes());
        svm.airdrop(&payer_addr, 1_000 * 1_000_000_000)
            .map_err(|e| BpfRunnerError::AddProgram(format!("payer airdrop failed: {e:?}")))?;

        Ok(Self {
            svm,
            program_id,
            payer,
            so_path,
        })
    }

    /// Convenience: load from the canonical `.so` path, falling back to
    /// the `PERCOLATOR_PROG_SO` env var if set.
    pub fn from_default() -> Result<Self, BpfRunnerError> {
        let env_override = std::env::var("PERCOLATOR_PROG_SO").ok();
        let path = match env_override {
            Some(p) => PathBuf::from(p),
            None => Self::default_so_path(),
        };
        Self::new(path)
    }

    /// Default `.so` path. Resolves relative to the workspace if a
    /// `bench/` cwd, absolute if env override.
    pub fn default_so_path() -> PathBuf {
        // bench/ is one level under perc5ive, so DEFAULT_SO_PATH from
        // bench/ resolves with one `..` prefix. We don't know our cwd
        // at runtime, so try both candidates.
        let from_bench = PathBuf::from("..").join(DEFAULT_SO_PATH);
        if from_bench.exists() {
            return from_bench;
        }
        PathBuf::from(DEFAULT_SO_PATH)
    }

    /// Borrow the underlying LiteSVM. Probes construct their own
    /// transactions and submit via this.
    pub fn svm(&mut self) -> &mut LiteSVM {
        &mut self.svm
    }

    /// The funded payer keypair.
    pub fn payer(&self) -> &Keypair {
        &self.payer
    }

    /// The percolator-prog program id (`Perco1ator…`).
    pub fn program_id(&self) -> Pubkey {
        self.program_id
    }

    /// The funded payer's address (for tx construction). Bridges from
    /// `solana_pubkey::Pubkey` to `solana_address::Address` via bytes.
    pub fn payer_address(&self) -> Pubkey {
        Pubkey::new_from_array(self.payer.pubkey().to_bytes())
    }

    /// Source path of the loaded `.so` — for diagnostics + JSONL output.
    pub fn so_path(&self) -> &Path {
        &self.so_path
    }

    /// Read an account from the SVM, returning `None` if it doesn't exist.
    pub fn get_account(&self, key: &Pubkey) -> Option<Account> {
        self.svm.get_account(key)
    }
}
