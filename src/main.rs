use grok::{patterns, Grok};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "grop", about = "A grok powered grep-like utility")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Input file, stdin if not present
    #[structopt(parse(from_os_str))]
    input: Option<PathBuf>,

    /// Output file, stdout if not present
    #[structopt(parse(from_os_str))]
    output: Option<PathBuf>,

    /// List available patterns
    #[structopt(short, long)]
    list_pattern: Option<Option<String>>,
}

fn main() {
    let opt = Opt::from_args();
    if let Some(target) = opt.list_pattern {
        let mut spatterns = patterns().to_vec();
        spatterns.sort_by(|a, b| a.0.cmp(b.0));
        match target {
            Some(target) => {
                for pattern in spatterns.iter() {
                    if pattern.0 == target {
                        println!("{}", pattern.1);
                        return;
                    }
                }
            }
            None => {
                for pattern in spatterns.iter() {
                    println!("{}", pattern.0);
                }
                return;
            }
        }
    }
}
