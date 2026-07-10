use clap::Parser;

use mdpub::cli::Cli;
use mdpub::runner::RealRunner;

fn main() {
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {e}");
            std::process::exit(1);
        }
    };
    match mdpub::run(cli, &mut RealRunner, &cwd) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}
