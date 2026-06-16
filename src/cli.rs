use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "A Ratatui frontend for QMK keymap workflows."
)]
pub struct Cli {
    /// Optional QMK firmware checkout directory.
    #[arg(long)]
    pub qmk_home: Option<PathBuf>,

    /// Keyboard name, for example: splitkb/kyria/rev3.
    #[arg(short, long)]
    pub keyboard: Option<String>,

    /// Keymap name, for example: default or your username.
    #[arg(short = 'm', long)]
    pub keymap: Option<String>,
}
