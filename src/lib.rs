use grok::{patterns, Grok, Matches, Pattern};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};
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
    #[structopt(short, long)]
    output_format: Option<String>,

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

    /// Merge specified field among rows by specifying starting pattern (format: `<field_name> <regexp>`)
    /// The field to merge
    #[structopt(long)]
    merge_field: Option<String>,

    /// Merge specified field among rows by specifying starting pattern (format: `<field_name> <regexp>`)
    #[structopt(long)]
    merge_pattern_start: Option<String>,

    /// Merge specified field among rows by specifying ending pattern (format: `<field_name> <regexp>`)
    #[structopt(long)]
    merge_pattern_end: Option<String>,

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
        println!("{}", list_pattern(&pattern_map, target)?);
        return Ok(());
    }

    // Compile pattern
    let pattern: Pattern;
    match opt.expression {
        Some(expression) => pattern = grok.compile(&expression, false)?,
        None => pattern = grok.compile("%{GREEDYDATA:all}", false)?,
    }

    let input: Box<dyn Read> = match opt.input {
        Some(file) => Box::new(File::open(file)?),
        None => Box::new(io::stdin()),
    };

    let mut output = BufWriter::new(io::stdout());

    if opt.merge_field.is_none() {
        process(input, Box::new(output), &pattern, &opt.output_format);
    }
    let mut in_scope = false;
    let mut buffer_m = HashMap::<String, String>::new();
    for line in BufReader::new(input).lines() {
        match line {
            Err(error) => return Err(Box::new(error)),
            Ok(line) => {
                if let Some(m) = process_line(&line, &pattern, &opt.output_format) {
                    match opt.merge_field {
                        Some(merge_field) => match opt.merge_pattern_start {
                            Some(merge_pattern_start) => match opt.merge_pattern_end {
                                Some(merge_pattern_end) => {}
                                None => {
                                    let match_start_pattern = match_merge_pattern(
                                        &m,
                                        &merge_field,
                                        &merge_pattern_start,
                                    )?;
                                    if !in_scope {
                                        if match_start_pattern {
                                            buffer_m = m;
                                        } else {
                                            output.write(line.as_bytes())?;
                                        }
                                    } else {
                                        if match_start_pattern {
                                            output.write(
                                                format_output(&buffer_m, &opt.output_format)
                                                    .as_bytes(),
                                            )?;
                                            buffer_m = m;
                                        } else {
                                            append_match(&mut buffer_m, &m, &merge_field);
                                        }
                                    }
                                }
                            },
                            None => {
                                append_match(&mut buffer_m, &m, &merge_field);
                            }
                        },
                        None => {
                            output.write(line.as_bytes())?;
                        }
                    }
                }
            }
        }
    }
    output.flush()?;
    Ok(())
}

fn process(
    input: Box<dyn Read>,
    output: Box<dyn Write>,
    pattern: &grok::Pattern,
    output_format: &Option<String>,
) -> Result<(), String> {
    for line in BufReader::new(input).lines() {
        match line {
            Err(error) => return Err(error.to_string()),
            Ok(line) => {
                if let Some(_) = process_line(&line, &pattern, &output_format) {
                    output.write(line.as_bytes());
                }
            }
        }
    }
    Ok(())
}

fn append_match(buf_m: &mut HashMap<String, String>, m: &HashMap<String, String>, field: &str) {
    buf_m.insert(
        String::from(field),
        format!("{}\n{}", buf_m.get(field).unwrap(), m.get(field).unwrap()),
    );
}

fn extract_pattern(p: &str) -> Result<(String, String), String> {
    let pt = p.splitn(2, " ").collect::<Vec<&str>>();
    if pt.len() != 2 {
        return Err(String::from(
            r#"Invalid pattern (should be "pattern_name regexp")"#,
        ));
    }
    Ok((String::from(pt[0]), String::from(pt[1])))
}

fn match_merge_pattern(
    m: &HashMap<String, String>,
    merge_field: &str,
    merge_pattern: &str,
) -> Result<bool, String> {
    match m.get(merge_field) {
        Some(payload) => {
            let p = Grok::default().compile(&merge_pattern, false);
            if let Err(err) = p {
                return Err(err.to_string());
            }
            let p = p.unwrap();
            let m = p.match_against(payload);
            if m.is_none() {
                return Ok(false);
            }
            Ok(true)
        }
        None => {
            return Err(format!("unknown filed: {}", merge_field));
        }
    }
}

fn add_pattern(grok: &mut Grok, m: &mut HashMap<String, String>, p: &str) -> Result<(), String> {
    let (field_name, field_pattern) = extract_pattern(p)?;
    m.insert(String::from(&field_name), String::from(&field_pattern));
    grok.insert_definition(String::from(&field_name), String::from(&field_pattern));
    Ok(())
}

fn list_pattern(
    pattern_map: &HashMap<String, String>,
    target_pattern: Option<String>,
) -> Result<String, String> {
    match target_pattern {
        Some(target) => match pattern_map.get(&target) {
            Some(v) => {
                return Ok(String::from(v));
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
            Ok(spatterns
                .iter()
                .map(|(k, _)| String::from(*k))
                .collect::<Vec<String>>()
                .join("\n"))
        }
    }
}

fn process_line(
    line: &str,
    pattern: &Pattern,
    oformat: &Option<String>,
) -> Option<HashMap<String, String>> {
    if let Some(m) = pattern.match_against(line) {
        // Convert Match to Map so that we can mutate the content for merging
        let mmap = m
            .iter()
            .map(|(k, v)| (String::from(k), String::from(v)))
            .collect::<HashMap<String, String>>();
        return Some(mmap);
    }
    None
}

fn format_output(m: &HashMap<String, String>, format: &Option<String>) -> String {
    match format {
        Some(format) => format
            .split(",")
            .map(|k| {
                String::from(
                    m.get(k)
                        .expect(&format!("unknown field in format string: {}", k)),
                )
            })
            .collect::<Vec<String>>()
            .join(" "),
        None => m
            .iter()
            .map(|x| String::from(x.0))
            .collect::<Vec<String>>()
            .join(" "),
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_add_valid_pattern() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "FOO foo").expect("failed to add pattern");
        assert_eq!(pattern_map.get("FOO").unwrap(), "foo");
        let p = grok
            .compile("%{FOO:foo}", true)
            .expect("failed to compile pattern");
        let m = p.match_against("foo").expect("failed to match pattern");
        assert_eq!(m.get("foo").unwrap(), "foo");
    }

    #[test]
    fn test_add_invalid_pattern() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        assert!(add_pattern(&mut grok, &mut pattern_map, "FOO,foo").is_err());
    }

    #[test]
    fn test_list_pattern() {
        let mut pattern_map = HashMap::<String, String>::new();
        pattern_map.insert(String::from("FOO"), String::from("foo"));
        pattern_map.insert(String::from("BAR"), String::from("bar"));
        assert_eq!(list_pattern(&pattern_map, None).unwrap(), "BAR\nFOO");
        assert_eq!(
            list_pattern(&pattern_map, Some(String::from("FOO"))).unwrap(),
            "foo"
        );
    }

    #[test]
    fn test_format_output() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "FOO foo").expect("failed to add pattern");
        add_pattern(&mut grok, &mut pattern_map, "BAR bar").expect("failed to add pattern");
        let p = grok
            .compile("%{FOO:foo} %{BAR:bar}", true)
            .expect("failed to compile pattern");
        let m = p.match_against("foo bar").expect("failed to match pattern");
        assert_eq!(format_output(&m, &Some(String::from("bar,foo"))), "bar foo");
    }

    #[test]
    fn test_process_line() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "FOO foo").expect("failed to add pattern");
        add_pattern(&mut grok, &mut pattern_map, "BAR bar").expect("failed to add pattern");
        let p = grok
            .compile("%{FOO:foo} %{BAR:bar}", true)
            .expect("failed to compile pattern");
        assert_eq!(
            process_line("foo bar", &p, &Some(String::from("foo,bar")))
                .expect("no output from process_line"),
            "foo bar"
        );
        assert!(process_line("bar", &p, &Some(String::from("foo,bar"))).is_none(),);
    }
}
