use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser, ValueEnum};

use crate::commands;

#[derive(Debug, Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
pub enum Command {
    #[command(about, author, version)]
    TreeTui(TreeArgs),
}

impl Command {
    pub fn exec(self) -> Result<()> {
        match self {
            Command::TreeTui(args) => commands::tree_tui::run(args),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Prefix {
    Depth,
    Indent,
    None,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Charset {
    Utf8,
    Ascii,
}

#[derive(Debug, Parser)]
pub struct TreeArgs {
    /// Deprecated, use --no-dedupe instead
    #[arg(short = 'a', long = "all", hide = true)]
    pub all: bool,

    /// Deprecated, use -e=no-dev instead
    #[arg(long = "no-dev-dependencies", hide = true)]
    pub no_dev_dependencies: bool,

    /// The kinds of dependencies to display
    #[arg(short = 'e', long = "edges", value_name = "KINDS", action = ArgAction::Append)]
    pub edges: Vec<String>,

    /// Invert the tree direction and focus on the given package
    #[arg(short = 'i', long = "invert", value_name = "SPEC", action = ArgAction::Append)]
    pub invert: Vec<String>,

    /// Prune the given package from the display of the dependency tree
    #[arg(long = "prune", value_name = "SPEC")]
    pub prune: Vec<String>,

    /// Maximum display depth of the dependency tree
    #[arg(long = "depth", value_name = "DEPTH")]
    pub depth: Option<usize>,

    /// Deprecated, use --prefix=none instead
    #[arg(long = "no-indent", hide = true)]
    pub no_indent: bool,

    /// Deprecated, use --prefix=depth instead
    #[arg(long = "prefix-depth", hide = true)]
    pub prefix_depth: bool,

    /// Change the prefix (indentation) of how each entry is displayed
    #[arg(long = "prefix", value_name = "PREFIX", value_enum, default_value_t = Prefix::Indent)]
    pub prefix: Prefix,

    /// Do not de-duplicate (repeats all shared dependencies)
    #[arg(long = "no-dedupe")]
    pub no_dedupe: bool,

    /// Show only dependencies which come in multiple versions (implies -i)
    #[arg(short = 'd', long = "duplicates", alias = "duplicate")]
    pub duplicates: bool,

    /// Character set to use in output
    #[arg(long = "charset", value_name = "CHARSET", value_enum)]
    pub charset: Option<Charset>,

    /// Format string used for printing dependencies
    #[arg(
        short = 'f',
        long = "format",
        value_name = "FORMAT",
        default_value = "{p}"
    )]
    pub format: String,

    /// Package to be used as the root of the tree
    #[arg(long = "package", value_name = "SPEC")]
    pub package: Vec<String>,

    /// Display the tree for all packages in the workspace
    #[arg(long = "workspace")]
    pub workspace: bool,

    /// Exclude specific workspace members
    #[arg(long = "exclude", value_name = "SPEC")]
    pub exclude: Vec<String>,

    /// Activate all available features
    #[arg(long = "all-features")]
    pub all_features: bool,

    /// Do not activate the `default` feature
    #[arg(long = "no-default-features")]
    pub no_default_features: bool,

    /// Space-separated list of features to activate
    #[arg(short = 'F', long = "features", value_delimiter = ',')]
    pub features: Vec<String>,

    /// Deprecated, use --target=all instead
    #[arg(long = "all-targets", hide = true)]
    pub all_targets: bool,

    /// Filter dependencies matching the given target-triple
    #[arg(long = "target", value_name = "TRIPLE", action = ArgAction::Append)]
    pub target: Vec<String>,

    /// Path to Cargo.toml
    #[arg(long = "manifest-path", value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// Path to Cargo.lock
    #[arg(long = "lockfile-path", value_name = "PATH")]
    pub lockfile_path: Option<PathBuf>,
}

#[test]
fn verify_app() {
    use clap::CommandFactory;
    Command::command().debug_assert();
}
