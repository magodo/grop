use grop::Opt;
use log;
use std::process::exit;
use structopt::StructOpt;

extern crate stderrlog;

fn main() {
    let opt = Opt::from_args();

    stderrlog::new()
        .verbosity(opt.verbose)
        .quiet(opt.quiet)
        .init()
        .unwrap();

    if let Err(err) = grop::run(opt) {
        log::error!("{}", err);
        exit(1);
    }
}
