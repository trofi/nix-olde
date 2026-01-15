pub use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};

/// A tool to show outdated packages in current system according to
/// repology.org database.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Opts {
    /// Alternative path to <nixpkgs> location.
    #[arg(short, long)]
    pub(crate) nixpkgs: Option<String>,

    /// Enable extra verbosity to report unexpected events,
    /// fetch progress and so on.
    #[command(flatten)]
    pub(crate) verbose: Verbosity<InfoLevel>,

    /// Pass a system flake alternative to /etc/nixos default.
    #[arg(short, long)]
    pub(crate) flake: Option<String>,

    /// Pass a repository to pull current versions from.
    /// Possible values are all "nixpkgs" flavours at
    /// https://repology.org/repositories/statistics.
    #[arg(short, long, default_value = "nix_unstable")]
    pub(crate) repology_repo: String,
}
