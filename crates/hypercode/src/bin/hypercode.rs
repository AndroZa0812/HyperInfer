//! `hypercode` — Interactive CLI and single-shot coding agent powered by HyperInfer.

use hypercode::cli::{run_cli, CliOptions};

#[tokio::main]
async fn main() {
    let args = std::env::args().skip(1);
    let opts = match CliOptions::parse_from(args) {
        Ok(o) => o,
        Err(err) => {
            eprintln!("\x1b[31;1mError:\x1b[0m {}\n", err);
            eprintln!("{}", CliOptions::help_text());
            std::process::exit(2);
        }
    };

    if let Err(err) = run_cli(opts).await {
        eprintln!("\x1b[31;1mError:\x1b[0m {}", err);
        std::process::exit(1);
    }
}
