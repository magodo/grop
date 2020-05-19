use grok::{patterns, Grok, Matches, Pattern};
use log;
use std::char;
use std::collections::HashMap;
use std::error;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug)]
pub enum GropError {
    Io(io::Error),
    Compile(grok::Error),
    InvalidArg(String),
}

impl fmt::Display for GropError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &*self {
            GropError::Io(err) => err.fmt(f),
            GropError::Compile(err) => err.fmt(f),
            GropError::InvalidArg(msg) => write!(f, "Invalid argument {}", msg),
        }
    }
}

impl error::Error for GropError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &*self {
            GropError::Io(err) => Some(err),
            GropError::Compile(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for GropError {
    fn from(err: io::Error) -> GropError {
        GropError::Io(err)
    }
}
impl From<grok::Error> for GropError {
    fn from(err: grok::Error) -> GropError {
        GropError::Compile(err)
    }
}

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

    /// Filter to include (`field_name pattern`) or exclude (`-field_name pattern`) some pattern
    #[structopt(long)]
    filter: Option<Vec<String>>,

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

    /// Silence all output
    #[structopt(short, long)]
    pub quiet: bool,

    /// Verbose mode (-v, -vv, -vvv, etc)
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: usize,
}

struct MatchWrapper<'a>(Matches<'a>);

impl<'a> From<Matches<'a>> for MatchWrapper<'a> {
    fn from(m: Matches<'a>) -> MatchWrapper<'a> {
        MatchWrapper(m)
    }
}

impl<'a> Into<HashMap<String, String>> for MatchWrapper<'a> {
    fn into(self) -> HashMap<String, String> {
        self.0
            .iter()
            .map(|(k, v)| (String::from(k), String::from(v)))
            .collect::<HashMap<String, String>>()
    }
}

pub fn run(opt: Opt) -> Result<(), GropError> {
    let mut grok = Grok::default();

    let mut pattern_map: HashMap<String, String> = patterns()
        .to_vec()
        .iter()
        .map(|(x, y)| (String::from(*x), String::from(*y)))
        .collect();

    // Read customized patterns (if any)
    if let Some(custom_pattern_file) = opt.pattern_file {
        for line in BufReader::new(File::open(custom_pattern_file)?).lines() {
            let line = line?;
            add_pattern(&mut grok, &mut pattern_map, &line)?;
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

    let input: Box<dyn Read> = match opt.input {
        Some(file) => Box::new(File::open(file)?),
        None => Box::new(io::stdin()),
    };
    let mut output = io::stdout();

    match (&opt.merge_field, &opt.merge_exp_start, &opt.merge_exp_end) {
        (None, None, None) => process(
            input,
            &mut output,
            &opt.expression,
            &opt.output_format,
            &opt.filter,
            &mut grok,
        ),
        (Some(merge_field), Some(merge_exp_start), Some(merge_exp_end)) => process_merge(
            input,
            &mut output,
            &opt.expression,
            &opt.output_format,
            &merge_field,
            &merge_exp_start,
            &merge_exp_end,
            opt.merge_scope_exclusive,
            &opt.filter,
            &mut grok,
        ),
        _ => Err(GropError::InvalidArg(format!(
            "invalid merge option combinations"
        ))),
    }
}

fn process(
    input: Box<dyn Read>,
    output: &mut dyn Write,
    exp: &Option<String>,
    oformat: &Option<String>,
    filter: &Option<Vec<String>>,
    mut grok: &mut Grok,
) -> Result<(), GropError> {
    let p: Pattern;
    match exp {
        Some(expression) => p = grok.compile(&expression, false)?,
        None => p = grok.compile("%{GREEDYDATA:all}", false)?,
    }

    for line in BufReader::new(input).lines() {
        let line = line?;
        if let Some(m) = p.match_against(&line) {
            if let Some(output_line) =
                format_output(&MatchWrapper::from(m).into(), &oformat, &filter, &mut grok)?
            {
                output.write(format!("{}\n", output_line,).as_bytes())?;
            }
        }
    }
    Ok(())
}

fn process_merge(
    input: Box<dyn Read>,
    output: &mut dyn Write,
    exp: &Option<String>,
    oformat: &Option<String>,
    merge_field: &Vec<String>,
    merge_exp_start: &str,
    merge_exp_end: &str,
    merge_scope_exclusive: bool,
    filter: &Option<Vec<String>>,
    mut grok: &mut Grok,
) -> Result<(), GropError> {
    let mut in_scope = false;

    let p: Pattern;
    match exp {
        Some(expression) => p = grok.compile(&expression, false)?,
        None => p = grok.compile("%{GREEDYDATA:all}", false)?,
    }
    let p_start = grok.compile(merge_exp_start, false)?;
    let p_end = grok.compile(merge_exp_end, false)?;
    let mut buf = HashMap::<String, String>::new();
    for line in BufReader::new(input).lines() {
        let line = line?;
        if let Some(m) = p.match_against(&line) {
            match (
                in_scope,
                p_start.match_against(&line),
                p_end.match_against(&line),
            ) {
                (false, None, _) => {
                    log::info!("process merge: regular line: {}", line);

                    if let Some(output_line) =
                        format_output(&MatchWrapper::from(m).into(), &oformat, &filter, &mut grok)?
                    {
                        output.write(format!("{}\n", output_line,).as_bytes())?;
                    }
                }
                (false, Some(_), _) => {
                    log::info!("process merge: entering merge scope: {}", line);
                    in_scope = true;
                    buf = MatchWrapper::from(m).into();
                }
                (true, _, None) => {
                    log::info!("process merge: in scope: {}", line);
                    merge_match_to_buf(&merge_field, &m, &mut buf)?;
                }
                (true, match_start, Some(_)) => {
                    if merge_scope_exclusive {
                        log::info!("process merge: entering merge scope (exclusive): {}", line);

                        if let Some(output_line) =
                            format_output(&buf, &oformat, &filter, &mut grok)?
                        {
                            output.write(format!("{}\n", output_line,).as_bytes())?;
                        }
                        buf.clear();

                        // In case the end expression is exclusive, we need further check if it
                        // match the start expression. If so, we will launch a new merge section
                        // right away.
                        if let Some(_) = match_start {
                            buf = MatchWrapper::from(m).into();
                            in_scope = true;
                        } else {
                            // Not match start expression, just output current line and clear
                            // buffer and state.
                            if let Some(output_line) = format_output(
                                &MatchWrapper::from(m).into(),
                                &oformat,
                                &filter,
                                &mut grok,
                            )? {
                                output.write(format!("{}\n", output_line,).as_bytes())?;
                            }
                            in_scope = false;
                        }
                    } else {
                        log::info!("process merge: entering merge scope (inclusive): {}", line);
                        merge_match_to_buf(&merge_field, &m, &mut buf)?;

                        if let Some(output_line) =
                            format_output(&buf, &oformat, &filter, &mut grok)?
                        {
                            output.write(format!("{}\n", output_line,).as_bytes())?;
                        }
                        buf.clear();
                        in_scope = false;
                    }
                }
            }
        }
    }
    Ok(())
}

fn merge_match_to_buf(
    merge_field: &Vec<String>,
    m: &Matches,
    buf: &mut HashMap<String, String>,
) -> Result<(), GropError> {
    for field in merge_field.iter() {
        buf.insert(
            String::from(field),
            match buf.get(field) {
                Some(o) => format!(
                    "{}\n{}",
                    o,
                    m.get(field)
                        .ok_or(GropError::InvalidArg(format!(
                            "merge_field {} not exists in pattern",
                            field
                        )))
                        .unwrap(),
                ),
                None => String::from(
                    m.get(field)
                        .ok_or(GropError::InvalidArg(format!(
                            "merge_field {} not exists in pattern",
                            field
                        )))
                        .unwrap(),
                ),
            },
        );
    }
    Ok(())
}

fn add_pattern(grok: &mut Grok, m: &mut HashMap<String, String>, p: &str) -> Result<(), GropError> {
    let pt = p
        .splitn(2, char::is_whitespace)
        .map(str::trim)
        .collect::<Vec<&str>>();
    if pt.len() != 2 {
        return Err(GropError::InvalidArg(String::from(
            r#"Invalid pattern (should be "pattern_name pattern")"#,
        )));
    }
    m.insert(String::from(pt[0]), String::from(pt[1]));
    grok.insert_definition(String::from(pt[0]), String::from(pt[1]));
    Ok(())
}

fn list_pattern(
    pattern_map: &HashMap<String, String>,
    target_pattern: Option<String>,
) -> Result<String, GropError> {
    match target_pattern {
        Some(target) => match pattern_map.get(&target) {
            Some(v) => {
                return Ok(String::from(v));
            }
            None => {
                return Err(GropError::InvalidArg(format!(
                    "Unknown target pattern {}",
                    &target
                )));
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

fn format_output(
    m: &HashMap<String, String>,
    format: &Option<String>,
    filter: &Option<Vec<String>>,
    grok: &mut Grok,
) -> Result<Option<String>, GropError> {
    if let Some(filters) = filter {
        let mut to_keep = true;
        // Iterate each filter in order to evaluate whether to keep or skip this match.
        for filter in filters.iter() {
            let (is_negative, filter) = if filter.starts_with("-") {
                (true, String::from(&filter[1..]))
            } else {
                (false, String::from(&filter[..]))
            };
            let ft = filter
                .splitn(2, char::is_whitespace)
                .map(str::trim)
                .collect::<Vec<&str>>();
            if ft.len() != 2 {
                return Err(GropError::InvalidArg(String::from(
                    r#"Invalid pattern (should be "field_name pattern")"#,
                )));
            }
            let (name, pattern) = (ft[0], ft[1]);
            log::info!(
                "name: {}, pattern: {}, negative: {}",
                name,
                pattern,
                is_negative
            );
            let field_payload = m.get(name).expect(&format!("unknown field: {}", name));
            if is_negative {
                if grok
                    .compile(pattern, false)?
                    .match_against(field_payload)
                    .is_some()
                {
                    to_keep = false;
                }
            } else {
                if grok
                    .compile(pattern, false)?
                    .match_against(field_payload)
                    .is_some()
                {
                    to_keep = true;
                }
            }
            log::info!("to_keep: {}", to_keep);
        }
        if !to_keep {
            return Ok(None);
        }
    }
    match format {
        Some(format) => Ok(Some(
            format
                .split(",")
                .map(|k| {
                    String::from(
                        m.get(k)
                            .expect(&format!("unknown field in format string: {}", k)),
                    )
                })
                .collect::<Vec<String>>()
                .join(" "),
        )),
        None => Ok(Some(
            m.iter()
                .map(|x| String::from(x.1))
                .collect::<Vec<String>>()
                .join(" "),
        )),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::io::Cursor;

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
        assert_eq!(
            format_output(
                &MatchWrapper::from(m).into(),
                &Some(String::from("bar,foo")),
                &None,
                &mut grok,
            )
            .unwrap()
            .unwrap(),
            "bar foo"
        );
    }

    #[test]
    fn test_format_output_filter() {
        let mut grok = Grok::default();
        let p = grok
            .compile("%{GREEDYDATA:data}", true)
            .expect("failed to compile pattern");
        let m = p.match_against("bar").expect("failed to match pattern");
        assert_eq!(
            format_output(
                &MatchWrapper::from(m).into(),
                &Some(String::from("data")),
                &Some(vec![String::from("-data ^b"), String::from("data bar")]),
                &mut grok,
            )
            .unwrap()
            .unwrap(),
            "bar",
        );
    }

    #[test]
    fn test_process() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "FOO foo").expect("failed to add pattern");
        add_pattern(&mut grok, &mut pattern_map, "BAR bar").expect("failed to add pattern");
        let exp = String::from("%{FOO:foo} %{BAR:bar}");

        let input = Cursor::new(
            r#"
foo bar
foo
bar
            "#
            .as_bytes(),
        );
        let mut output = Cursor::new(Vec::new());
        process(
            Box::new(input),
            &mut output,
            &Some(exp),
            &Some(String::from("foo,bar")),
            &None,
            &mut grok,
        )
        .expect("failed to process");
        assert_eq!(&output.get_ref()[..], "foo bar\n".as_bytes())
    }

    #[test]
    fn test_process_merge_inclusive() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "PREFIX =").expect("failed to add pattern");
        let exp = String::from("%{PREFIX:prefix} %{GREEDYDATA:greedydata}");

        let input = Cursor::new(
            r#"
= 1
= START 2
= 3
= END 4
= 5
            "#
            .as_bytes(),
        );
        let mut output = Cursor::new(Vec::new());
        process_merge(
            Box::new(input),
            &mut output,
            &Some(exp),
            &Some(String::from("prefix,greedydata")),
            &vec![String::from("greedydata")],
            "%{PREFIX} START",
            "%{PREFIX} END",
            false,
            &None,
            &mut grok,
        )
        .expect("failed to process");
        log::info!("{:?}", std::str::from_utf8(output.get_ref()));
        assert_eq!(
            &output.get_ref()[..],
            r#"= 1
= START 2
3
END 4
= 5
"#
            .as_bytes()
        );
    }

    #[test]
    fn test_process_merge_exclusive() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "PREFIX =").expect("failed to add pattern");
        let exp = String::from("%{PREFIX:prefix} %{GREEDYDATA:greedydata}");

        let input = Cursor::new(
            r#"
= 1
= REQUEST
= 2
= RESPONSE
= 3
            "#
            .as_bytes(),
        );
        let mut output = Cursor::new(Vec::new());
        process_merge(
            Box::new(input),
            &mut output,
            &Some(exp),
            &Some(String::from("prefix,greedydata")),
            &vec![String::from("greedydata")],
            "%{PREFIX} REQUEST",
            "%{PREFIX} RESPONSE",
            true,
            &None,
            &mut grok,
        )
        .expect("failed to process");
        log::info!("{:?}", std::str::from_utf8(output.get_ref()));
        assert_eq!(
            &output.get_ref()[..],
            r#"= 1
= REQUEST
2
= RESPONSE
= 3
"#
            .as_bytes()
        );
    }

    #[test]
    fn test_process_merge_exclusive_continue() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "PREFIX =").expect("failed to add pattern");
        let exp = String::from("%{PREFIX:prefix} %{GREEDYDATA:greedydata}");

        let input = Cursor::new(
            r#"
= DEBUG 1
= DEBUG REQUEST
= 2
= DEBUG RESPONSE
= 3
= DEBUG 4
            "#
            .as_bytes(),
        );
        let mut output = Cursor::new(Vec::new());
        process_merge(
            Box::new(input),
            &mut output,
            &Some(exp),
            &Some(String::from("prefix,greedydata")),
            &vec![String::from("greedydata")],
            "%{PREFIX} DEBUG REQUEST|RESPONSE",
            "%{PREFIX} DEBUG",
            true,
            &None,
            &mut grok,
        )
        .expect("failed to process");
        log::info!("{:?}", std::str::from_utf8(output.get_ref()));
        assert_eq!(
            &output.get_ref()[..],
            r#"= DEBUG 1
= DEBUG REQUEST
2
= DEBUG RESPONSE
3
= DEBUG 4
"#
            .as_bytes()
        );
    }

    #[test]
    fn test_process_merge_exclusive_continue_filter() {
        let mut grok = Grok::default();
        let mut pattern_map = HashMap::<String, String>::new();
        add_pattern(&mut grok, &mut pattern_map, "PREFIX =").expect("failed to add pattern");
        let exp = String::from("%{PREFIX:prefix} %{GREEDYDATA:greedydata}");

        let input = Cursor::new(
            r#"
= DEBUG 1
= DEBUG REQUEST
= 2
= n
= y
= DEBUG RESPONSE
= 3
= n
= 4
= DEBUG 5
            "#
            .as_bytes(),
        );
        let mut output = Cursor::new(Vec::new());
        process_merge(
            Box::new(input),
            &mut output,
            &Some(exp),
            &Some(String::from("prefix,greedydata")),
            &vec![String::from("greedydata")],
            "%{PREFIX} DEBUG REQUEST|RESPONSE",
            "%{PREFIX} DEBUG",
            true,
            &Some(vec![
                String::from("-greedydata n"),
                String::from("greedydata y"),
            ]),
            &mut grok,
        )
        .expect("failed to process");
        log::info!("{:?}", std::str::from_utf8(output.get_ref()));
        assert_eq!(
            &output.get_ref()[..],
            r#"= DEBUG 1
= DEBUG REQUEST
2
n
y
= DEBUG 5
"#
            .as_bytes()
        );
    }
}
