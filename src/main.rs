use ::time::Duration;
use anyhow::{Context, Result, anyhow};
use clap::{Parser, command};
use regex::Regex;
use rust_decimal::Decimal;
use serde::Serialize;
use std::{
    collections::HashMap,
    fmt::Debug,
    fs::File,
    io::{BufRead, BufReader, BufWriter},
    path::Path,
};
use trace::Trace;
use yansi::{Condition, Paint};

mod device;
mod trace;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Run servo on an open harmony device and collect timing information
struct Args {
    #[arg(short, long)]
    /// Show all traces for servo
    all_traces: bool,

    /// The number of tries we should have to average
    #[arg(short = 'n', long, default_value_t = 1)]
    tries: usize,

    /// The homepage we try to load
    #[arg(short = 'p', long, default_value_t = String::from("https://servo.org"))]
    homepage: String,

    /// Trace Buffer size in KB
    #[arg(short = 't', long, default_value_t = 524288)]
    trace_buffer: u64,

    /// Number of sleep seconds
    #[arg(short, long, default_value_t = 10)]
    sleep: u64,

    /// Stay silent and only return the miliseconds in a list
    #[arg(short, long, default_value_t = false)]
    computer_output: bool,

    /// Name of the app bundle to start
    #[arg(short, long, default_value_t = String::from("org.servo.servo"))]
    bundle_name: String,

    /// Use Bencher output format
    #[arg(long, default_value_t = true)]
    bencher: bool,
}

/// Read a file into traces
fn read_file(args: &Args, f: &Path) -> Result<Vec<Trace>> {
    // This is more specific servo tracing with the tracing_mark_write
    // Example trace: `org.servo.servo-44962   (  44682) [010] .... 17864.716645: tracing_mark_write: B|44682|ML: do_single_part3_compilation`
    let bundle_short = args.bundle_name.rsplit('.').next().ok_or(anyhow!("Your bundle name does not have a dot. We need a dot because hitrace sometimes does not show the whole bundle name"))?;
    let regex = Regex::new(&format!(
        r"^.(.*?{}.*?)\-(\d+)\s*\(\s*(\d+)\).*?(\d+)\.(\d+): tracing_mark_write: (.)\|(\d+?)\|(.*?):(.*?)\s*$",
        &bundle_short
    ))?;
    let f = File::open(f)?;
    let reader = BufReader::new(f);

    let (valid_lines, invalid_lines): (Vec<_>, Vec<_>) = reader
        .lines()
        .enumerate()
        .partition(|(_index, l)| l.is_ok());

    if !invalid_lines.is_empty() {
        println!(
            "Could not read lines {:?}",
            invalid_lines
                .iter()
                .map(|(index, _l)| index)
                .collect::<Vec<_>>()
        );
    }

    valid_lines
        .into_iter()
        .filter_map(|(_index, l)| line_to_trace(&regex, &l.unwrap()))
        .collect::<Result<Vec<Trace>>>()
        .context("Could not parse one thing")
}

/// There is always one trace per line
/// This means that having no matched lines is ok and returns None. Having a parsing error returns Some(Err)
fn line_to_trace(regex: &Regex, line: &str) -> Option<Result<Trace>> {
    regex
        .captures_iter(line)
        .map(|c| c.extract())
        .map(trace::match_to_trace)
        .next()
}

#[derive(Debug, Serialize)]
/// the difference in timing, represented by two integers, representing major and minor difference
struct Difference<'a> {
    /// Major and minor differences
    difference: Duration,
    /// The name of the difference
    name: &'a str,
}

/// Way to construct filters
struct Filter<'a> {
    /// A name for the filter that will be output
    name: &'a str,
    /// A function taking a trace and deciding if it should be the start of the timing
    first: fn(&Trace) -> bool,
    /// A function taking a trace and deciding if it should be the end of the timing
    last: fn(&Trace) -> bool,
}

/// Calculates the timestamp difference equaivalent to trace1-trace2
fn difference_of_traces(trace1: &Trace, trace2: &Trace) -> Duration {
    Duration::new(
        trace1.timestamp.seconds as i64 - trace2.timestamp.seconds as i64,
        (trace1.timestamp.micro as i32 - trace2.timestamp.micro as i32) * 1000,
    )
}

/// Look through the traces and find all timing differences coming from the filters
fn find_and_collect_notable_differences<'a>(
    args: &Args,
    v: &[Trace],
    filters: &'a Vec<Filter>,
) -> Result<HashMap<&'a str, Vec<Duration>>> {
    let mut differences = Vec::new();

    for f in filters {
        let first = v.iter().filter(|t| (f.first)(t)).collect::<Vec<&Trace>>();
        let last = v.iter().filter(|t| (f.last)(t)).collect::<Vec<&Trace>>();

        if first.len() != args.tries || last.len() != args.tries {
            return Err(anyhow!(
                "Your filter functions are not specific or over specific, we got the following number of results: name: {}, first: {}, last: {}",
                f.name,
                first.len(),
                last.len()
            ));
        } else {
            for (first, last) in first.iter().zip(last.iter()) {
                differences.push(Difference {
                    name: f.name,
                    difference: difference_of_traces(last, first),
                })
            }
        }
    }
    let mut hash: HashMap<&str, Vec<Duration>> = HashMap::new();
    for i in &differences {
        hash.entry(i.name)
            .and_modify(|v| v.push(i.difference))
            .or_insert(vec![(i.difference)]);
    }
    Ok(hash)
}

/// Print the differences
fn print_differences(args: &Args, hash: HashMap<&str, Vec<Duration>>, first: &Trace, last: &Trace) {
    println!(
        "First stamp {}, last stamp {}",
        first.timestamp, last.timestamp
    );
    println!(
        "----name {} {} {}------({}) runs (hp:{})------------------------",
        "avg".yellow(),
        "min".green(),
        "max".red(),
        args.tries,
        args.homepage
    );
    for (key, val) in hash.iter() {
        let avg = val
            .iter()
            .sum::<Duration>()
            .checked_div(args.tries as i32)
            .unwrap();
        let min = val.iter().min().unwrap();
        let max = val.iter().max().unwrap();
        println!(
            "{}: {} {} {}",
            key,
            avg.yellow().whenever(Condition::TTY_AND_COLOR),
            min.green().whenever(Condition::TTY_AND_COLOR),
            max.red().whenever(Condition::TTY_AND_COLOR)
        );
    }
}

/// Print the differences in computer format
fn print_computer(hash: HashMap<&str, Vec<Duration>>) {
    for (key, items) in hash.iter() {
        print!("{key}: ");
        for i in items {
            print!("{}.{}, ", i.whole_seconds(), i.whole_microseconds())
        }
        println!();
    }
}

#[derive(Serialize)]
struct latency {
    #[serde(with = "rust_decimal::serde::float")]
    value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    lower_value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    upper_value: Decimal,
}

/// Output in bencher json format
fn print_bencher(hash: HashMap<&str, Vec<Duration>>) {
    let b: HashMap<&str, HashMap<&str, latency>> = hash
        .into_iter()
        .map(|(key, dur_vec)| {
            let number = dur_vec[0].whole_seconds() * 100 + dur_vec[0].whole_milliseconds() as i64;
            let value = Decimal::new(number, 3);
            let mut map = HashMap::new();
            map.insert(
                "latency",
                latency {
                    value,
                    lower_value: value,
                    upper_value: value,
                },
            );
            (key, map)
        })
        .collect();
    //let r = serde_json::to_string(&b).unwrap();
    //println!("{}", r);
    let file = File::create("bench.json").expect("Could not create file");
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &b).expect("Could not write json");
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !device::is_device_reachable().context("Testing reachability of device")? {
        return Err(anyhow!("No phone seems to be reachable"));
    }

    ctrlc::set_handler(move || {
        device::stop_tracing(args.trace_buffer).expect("Could not stop tracing");
    })?;

    let mut traces = Vec::new();
    for i in 1..args.tries + 1 {
        if !args.bencher {
            println!("Running test {}", i);
        }
        let log_path = device::exec_hdc_commands(&args)?;
        let mut new_traces = read_file(&args, &log_path)?;
        traces.append(&mut new_traces);
    }

    let filters = vec![
        //Filter {
        //    name: "Startup",
        //    first: |t| t.shorthand == "H" && t.function.contains("InitServoCalled"),
        //    last: |t| t.shorthand == "H" && t.function.contains("PageLoadEndedPrompt"),
        //},
        Filter {
            name: "Surface->LoadStart",
            first: |t| t.shorthand == "H" && t.function.contains("on_surface_created_cb"),
            last: |t| t.shorthand == "H" && t.function.contains("load status changed Started"),
        },
        Filter {
            name: "Load->Compl",
            first: |t| t.shorthand == "H" && t.function.contains("load status changed Started"),
            last: |t| t.shorthand == "H" && t.function.contains("PageLoadEndedPrompt"),
        },
    ];

    if args.all_traces {
        println!("Printing {} traces", &traces.len());
        for i in &traces {
            println!("{:?}", i);
        }
        println!("----------------------------------------------------------\n\n");
    }

    let differences = find_and_collect_notable_differences(&args, &traces, &filters).context(
        "Something went wrong with finding the traces, look up to see what probably went wrong",
    )?;

    let first = traces.first().unwrap();
    let last = traces.last().unwrap();
    if args.computer_output {
        print_computer(differences);
    } else if args.bencher {
        print_bencher(differences);
    } else {
        print_differences(&args, differences, first, last);
    }

    Ok(())
}
