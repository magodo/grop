use grok::{patterns, Grok, Matches, Pattern};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "grop", about = "A grok powered grep-like utility")]
pub struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Input file, stdin if not present
    #[structopt(parse(from_os_str))]
    input: Option<PathBuf>,

    /// Output format (fields of grok expression, separated by comma)
    #[structopt(short = "o", long)]
    format: Option<String>,

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
    pub quiet: bool,

    /// Verbose mode (-v, -vv, -vvv, etc)
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: usize,
}

pub fn run(opt: Opt) -> Result<(), Box<dyn Error>> {
    let mut grok = Grok::default();

    let mut pattern_map: HashMap<String, String> = patterns()
        .to_vec()
        .iter()
        .map(|(x, y)| (String::from(*x), String::from(*y)))
        .collect();

    // Read customized patterns (if any)
    if let Some(custom_pattern_file) = opt.pattern_file {
        for line in BufReader::new(File::open(custom_pattern_file)?).lines() {
            if let Ok(p) = line {
                add_pattern(&mut grok, &mut pattern_map, &p)?;
            }
        }
    }
    if let Some(custom_patterns) = opt.pattern {
        for p in custom_patterns.iter() {
            add_pattern(&mut grok, &mut pattern_map, p)?;
        }
    }

    // List pattern
    if let Some(target) = opt.list_pattern {
        list_pattern(&pattern_map, target)?;
        return Ok(());
    }

    // Compile pattern
    let pattern: Pattern;
    match opt.expression {
        Some(expression) => pattern = grok.compile(&expression, true)?,
        None => pattern = grok.compile("%{GREEDYDATA:all}", true)?,
    }

    output(&opt.input, pattern, &opt.format)
}

fn add_pattern(
    grok: &mut Grok,
    m: &mut HashMap<String, String>,
    p: &str,
) -> Result<(), &'static str> {
    let pt = p.splitn(2, " ").collect::<Vec<&str>>();
    if pt.len() != 2 {
        return Err(r#"Invalid pattern (should be "pattern_name regexp")"#);
    }
    m.insert(String::from(pt[0]), String::from(pt[1]));
    grok.insert_definition(String::from(pt[0]), String::from(pt[1]));
    Ok(())
}

fn list_pattern(
    pattern_map: &HashMap<String, String>,
    target_pattern: Option<String>,
) -> Result<(), String> {
    match target_pattern {
        Some(target) => match pattern_map.get(&target) {
            Some(v) => {
                println!("{}", v);
                return Ok(());
            }
            None => {
                return Err(format!("Unknown pattern {}", &target));
            }
        },
        None => {
            let mut spatterns: Vec<(&str, &str)> = Vec::new();
            for item in pattern_map.iter() {
                spatterns.push((item.0, item.1));
            }
            spatterns.sort_by(|a, b| a.0.cmp(b.0));
            for pattern in spatterns.iter() {
                println!("{}", pattern.0);
            }
            return Ok(());
        }
    }
}

fn output(
    path: &Option<PathBuf>,
    pattern: Pattern,
    format: &Option<String>,
) -> Result<(), Box<dyn Error>> {
    let input: Box<dyn Read> = match path {
        Some(file) => Box::new(File::open(file)?),
        None => Box::new(io::stdin()),
    };
    for line in BufReader::new(input).lines() {
        if let Ok(l) = line {
            if let Some(m) = pattern.match_against(&l) {
                println!("{}", format_output(m, &format));
            }
        }
    }
    Ok(())
}

fn format_output(m: Matches, format: &Option<String>) -> String {
    let mut out = String::new();
    match format {
        Some(format) => {
            for k in format.split(",") {
                if let Some(v) = m.get(k) {
                    out.push_str(format!("{} ", v).as_str());
                }
            }
            out
        }
        None => {
            for x in m.iter() {
                if let Some(v) = m.get(x.0) {
                    out.push_str(format!("{} ", v).as_str());
                }
            }
            out
        }
    }
}
