# Grop

Grop is a `grok` powered `grep`-like CLI utility, that allows user to manipulate `grok` separated columns in different ways (E.g. filter rows with columns).

## Install

```bash
cargo install grop
```

## Usage

```bash
A grok powered grep-like utility

USAGE:
    grop [FLAGS] [OPTIONS] [--] [input]

FLAGS:
    -h, --help                     Prints help information
        --merge-scope-exclusive    Whether to take the line matching `merge_exp_end` as part of the merged section
    -q, --quiet                    Silence all output
    -V, --version                  Prints version information
    -v, --verbose                  Verbose mode (-v, -vv, -vvv, etc)

OPTIONS:
        --config <config-file>                 Config file in toml format. A sample file could be found at
                                               "doc/sample.toml"
    -e, --expression <expression>              Grok match expression
        --filter <filter>...                   Filter to include (`field_name pattern`) or exclude (`-field_name
                                               pattern`) some pattern
    -l, --list-pattern <list-pattern>          List available patterns
        --merge-exp-end <merge-exp-end>        Grok match expression indicating the end of the merged section
        --merge-exp-start <merge-exp-start>    Grok match expression indicating the start of the merged section
    -m, --merge-field <merge-field>...         Field(s) to be merged among lines. The unspecified fields will be skipped
                                               and only keep the ones in first line
    -o, --output-format <output-format>        Output format (fields of grok expression, separated by comma)
    -p, --pattern <pattern>...                 Custom Grok pattern (format: `<pattern_name> <regexp>`)

ARGS:
    <input>    Input file, stdin if not present
```

## Motivation

The terraform-provider-azurerm will output a log of logs and I want some way to filter away the uninterested logs.

Some of the log is easy to filter with tools like `grep -v`, as long as those logs are one line span.

However, because of the "auto split line" feature of terraform, some log spawned from Azure Go SDK is a payload with multiple lines. This kind of log will be split into multiple logs, prefixed with some terraform log formatting (e.g. timestamp, log level, .etc). This feature is fine when you are looking through the whole log file via terminal/file. However, it makes tools like `grep -v` useless when you want some filtering, since `grep` is line-based tool, it has no knowledge about the completeness of a payload which spans multiple lines.

This is where `grop` can help! `grop` works in a pipeline style:

1. Structure the input using grok
2. Merge one or more structured fields based on merging patterns (starting line pattern and ending line pattern, supporting inclusive merge and exclusive merge)
3. Filter in/out based on (merged) field level pattern
4. Format output (e.g. pick up the interested fields only)

Following is an example snippet showing how to filter aways some uninteresting log for terraform-provider-azurerm:

```bash
$ TF_LOG=DEBUG terraform plan 2>&1 | tee /tmp/tf.log | \
    grop -p "LOGLEVEL DEBUG|INFO|WARN|ERROR" \
         -p "PROVIDER_SUBJECT plugin.terraform-provider-azurerm" \
         -e "%{TIMESTAMP_ISO8601:ts} \[%{LOGLEVEL:lvl}\] %{PROVIDER_SUBJECT}: %{GREEDYDATA:data}" \
         -m data \
         --merge-exp-start=".* AzureRM Request|Response" \
         --merge-exp-end="%{TIMESTAMP_ISO8601:ts} \[%{LOGLEVEL:lvl}\] %{PROVIDER_SUBJECT}: \[DEBUG\]" \
         --merge-scope-exclusive \
         --filter="-data \[DEBUG\] AzureRM Client User Agent" \
         --filter="-data \[DEBUG\] Registering" \
         -o ts,data
```
