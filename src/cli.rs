#[allow(unused_imports)]
use clap::CommandFactory; // FIXME: This import is used and can't be removed
use clap::{Parser, value_parser};
use std::path::PathBuf;

/// Chess.com bulk game downloader. By default downloads all time controls and does not sort the games into different files based on time control.
#[derive(Parser, Clone)]
#[command(version = "0.4.0", name = "chess_dl", author = "Nimrod Hajaj")]
pub struct Options {
    // Make struct public
    #[arg()]
    pub usernames: Vec<String>, // Make fields public as needed by main
    /// Output directory.
    #[arg(short, default_value("."), value_parser(value_parser!(PathBuf)))]
    pub output_dir: PathBuf,

    #[arg(long, display_order = 3)]
    /// Include Blitz games.
    pub blitz: bool,

    /// Include Bullet games.
    #[arg(long, display_order = 2)]
    pub bullet: bool,

    /// Include Rapid games.
    #[arg(long, display_order = 4)]
    pub rapid: bool,
    /// Include Daily games.
    #[arg(long, display_order = 5)]
    pub daily: bool,

    /// Group games regardless of player color.
    #[arg(short = 'c', long, display_order = 6)]
    pub group_colors: bool,

    /// Group games regardless of username.
    #[arg(short = 'u', long, display_order = 7)]
    pub group_users: bool,

    /// Separate games by time control.
    #[arg(short = 't', long, group = "time")]
    pub separate_time: bool,

    /// Download raw PGN files without parsing or sorting.
    #[arg(long, conflicts_with_all(&["blitz", "bullet", "rapid", "daily", "separate_time"]))]
    pub raw: bool,

    /// Maximum number of concurrent archive downloads.
    #[arg(short = 'C', long, default_value("10"))]
    pub concurrent: usize,

    /// Total time limit for the program in minutes.
    #[arg(short = 'T', long)]
    pub time_limit: Option<u64>,
}
