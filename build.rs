use clap_complete::aot::generate_to;
use clap_complete::shells::Shell;
use clap::ValueEnum;
use std::env;
use std::io::Error;

// Include the cli module to access the Options struct
include!("src/cli.rs");

fn main() -> Result<(), Error> {
    let outdir = match env::var_os("OUT_DIR") {
        None => return Ok(()), // Skip if OUT_DIR is not set (e.g., during cargo check)
        Some(outdir) => outdir,
    };

    let mut cmd = Options::command(); // Get the command structure
    let name = cmd.get_name().to_string();

    // Generate completions for all supported shells
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, name.clone(), &outdir)?;
    }

    Ok(())
}