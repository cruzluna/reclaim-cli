#[allow(dead_code)]
#[path = "../cli.rs"]
mod cli;

use clap::{CommandFactory, Parser};
use std::{error::Error, fs, path::PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "reclaim-man",
    about = "Generate a reclaim(1) man page from the CLI definition."
)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "man/reclaim.1",
        help = "Where to write the generated man page."
    )]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let command = cli::Cli::command();
    let man = clap_mangen::Man::new(command);
    let mut buffer = Vec::new();
    man.render(&mut buffer)?;

    fs::write(&args.output, buffer)?;
    eprintln!("Wrote {}", args.output.display());

    Ok(())
}
