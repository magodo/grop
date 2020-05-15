use grok::{patterns, Grok, Pattern};
use log;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

extern crate stderrlog;

#[derive(Debug, StructOpt)]
#[structopt(name = "grop", about = "A grok powered grep-like utility")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Input file, stdin if not present
    #[structopt(parse(from_os_str))]
    input: Option<PathBuf>,

    /// List available patterns
    #[structopt(short, long)]
    list_pattern: Option<Option<String>>,

    /// Grok match expression
    #[structopt(short, long)]
    expression: Option<String>,

    /// Custom Grok pattern (format: `<pattern_name> <regexp>`)
    #[structopt(short, long)]
    pattern: Option<Vec<String>>,

    /// Custom Grok pattern file
    #[structopt(long, parse(from_os_str))]
    pattern_file: Option<PathBuf>,

    /// Silence all output
    #[structopt(short, long)]
    quiet: bool,

    /// Verbose mode (-v, -vv, -vvv, etc)
    #[structopt(short, long, parse(from_occurrences))]
    verbose: usize,
}

fn main() {
    let opt = Opt::from_args();

    stderrlog::new()
        .verbosity(opt.verbose)
        .quiet(opt.quiet)
        .init()
        .unwrap();

    let mut grok = Grok::default();

    let mut pattern_map: HashMap<String, String> = patterns()
        .to_vec()
        .into_iter()
        .map(|(x, y)| (String::from(x), String::from(y)))
        .collect();

    // Read customized patterns (if any)
    if let Some(custom_pattern_file) = opt.pattern_file {
        let file = File::open(custom_pattern_file).unwrap();
        for line in BufReader::new(file).lines() {
            if let Ok(p) = line {
                add_pattern(&mut grok, &mut pattern_map, &p);
            }
        }
    }
    if let Some(custom_patterns) = opt.pattern {
        for p in custom_patterns.iter() {
            add_pattern(&mut grok, &mut pattern_map, p);
        }
    }

    // List pattern
    if let Some(target) = opt.list_pattern {
        let mut spatterns: Vec<(&str, &str)> = Vec::new();
        for item in pattern_map.iter() {
            spatterns.push((item.0, item.1));
        }
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

    // Compile pattern
    let pattern: Pattern;
    match opt.expression {
        Some(expression) => {
            pattern = grok
                .compile(&expression, true)
                .expect("Error while compiling pattern")
        }
        // TODO: if no expression is passed, we shall simply echo
        None => panic!("TODO"),
    }

    // Filter and output
    let input: Box<dyn Read> = match opt.input {
        Some(file) => Box::new(File::open(file).unwrap()),
        None => Box::new(io::stdin()),
    };
    for line in BufReader::new(input).lines() {
        if let Ok(l) = line {
            if let Some(m) = pattern.match_against(&l) {
                // TODO: Output with order specified in expression
            }
        }
    }
}

fn add_pattern(grok: &mut Grok, m: &mut HashMap<String, String>, p: &str) {
    let pt = p.splitn(2, " ").collect::<Vec<&str>>();
    if pt.len() != 2 {
        panic!(r#"Invalid pattern (should be "pattern_name regexp")"#);
    }
    m.insert(String::from(pt[0]), String::from(pt[1]));
    grok.insert_definition(String::from(pt[0]), String::from(pt[1]));
}
