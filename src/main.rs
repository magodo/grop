use grop::{Config, MergeConfig};
use log;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

extern crate stderrlog;

#[derive(Debug, StructOpt, Deserialize)]
#[structopt(name = "grop", about = "A grok powered grep-like utility")]
pub struct Opt {
    /// Input file, stdin if not present
    #[structopt(parse(from_os_str))]
    input: Option<PathBuf>,

    /// Custom Grok pattern (format: `<pattern_name> <regexp>`)
    #[structopt(short, long)]
    pattern: Option<Vec<String>>,

    /// List available patterns
    #[structopt(short, long)]
    list_pattern: Option<Option<String>>,

    /// Grok match expression
    #[structopt(short, long)]
    expression: Option<String>,

    /// Field(s) to be merged among lines.
    /// The unspecified fields will be skipped and only keep the ones in first line.
    #[structopt(short, long, requires_all=&["merge-exp-start", "merge-exp-end"])]
    merge_field: Option<Vec<String>>,

    /// Grok match expression indicating the start of the merged section
    #[structopt(long, requires_all=&["merge-exp-end", "merge-field"])]
    merge_exp_start: Option<String>,

    /// Grok match expression indicating the end of the merged section
    #[structopt(long, requires_all=&["merge-exp-start", "merge-field"])]
    merge_exp_end: Option<String>,

    /// Whether to take the line matching `merge_exp_end` as part of the merged section
    #[structopt(long)]
    merge_scope_exclusive: bool,

    /// Filter to include (`field_name pattern`) or exclude (`-field_name pattern`) some pattern
    #[structopt(long)]
    filter: Option<Vec<String>>,

    /// Output format (fields of grok expression, separated by comma)
    #[structopt(short, long)]
    output_format: Option<String>,

    /// Silence all output
    #[structopt(short, long)]
    pub quiet: bool,

    /// Verbose mode (-v, -vv, -vvv, etc)
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Config file in toml format. A sample file could be found at "doc/sample.toml".
    #[structopt(long = "config", parse(from_os_str))]
    config_file: Option<PathBuf>,
}

impl Into<Config> for Opt {
    fn into(self) -> Config {
        Config {
            input: self.input,
            custom_patterns: self.pattern,
            list_pattern: self.list_pattern,
            match_expression: self.expression,
            merge_config: match (
                &self.merge_field,
                &self.merge_exp_start,
                &self.merge_exp_end,
            ) {
                (None, None, None) => None,
                _ => Some(MergeConfig {
                    merge_fields: self.merge_field,
                    merge_exp_start: self.merge_exp_start,
                    merge_exp_end: self.merge_exp_end,
                    merge_scope_exclusive: self.merge_scope_exclusive,
                }),
            },
            filters: self.filter,
            output_format: self.output_format,
        }
    }
}

fn main() {
    let opt = Opt::from_args();

    stderrlog::new()
        .verbosity(opt.verbose)
        .quiet(opt.quiet)
        .init()
        .unwrap();

    let config: Config;

    if let Some(config_file) = &opt.config_file {
        let content = fs::read_to_string(config_file).expect("failed to read config file");
        let cfg: Config = toml::from_str(&content).expect("failed to parse config file");
        config = cfg.merge(opt.into());
    } else {
        config = opt.into();
    }

    if let Err(err) = grop::run(config) {
        log::error!("{}", err);
        exit(1);
    }
}
