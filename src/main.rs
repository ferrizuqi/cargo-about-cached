mod app;
mod cache;
mod cli;
mod gather;
mod generate;
mod workspace;

use app::App;
use cli::Cli;

fn main() {
    if let Err(error) = App::new(Cli::parse_env()).run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
