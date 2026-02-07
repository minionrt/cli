mod api;
mod cli;
mod config;
mod context;
mod providers;
mod runtime;
mod util;

pub fn main() {
    cli::exec();
}
