use anyhow::{Context, Result, anyhow};
use args::Args;
use clap::Parser;
use filter::Filter;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use time::Duration;
use trace::Trace;
use yansi::{Condition, Paint};

mod args;
mod bencher;
mod device;
mod filter;
mod trace;

struct AvgMingMax {
    avg: Duration,
    min: Duration,
    max: Duration,
    number: usize,
}

fn avg_min_max(durations: &[Duration]) -> Option<AvgMingMax> {
    let number = durations.len();
    durations
        .iter()
        .min()
        .zip(durations.iter().max())
        .map(|(min, max)| AvgMingMax {
            avg: durations.iter().sum::<Duration>() / number as f64,
            min: *min,
            max: *max,
            number,
        })
}

/// Print the differences
fn print_differences(args: &Args, results: RunResults, errors: HashMap<&str, u32>) {
    println!("The following things broke with errors");
    for (key, val) in errors.iter() {
        println!("{}: {} errors", key, val);
    }

    println!(
        "----name {} {} {}------({}) runs (hp:{})------------------------",
        "avg".yellow(),
        "min".green(),
        "max".red(),
        args.tries,
        args.url
    );
    for (key, val) in results.iter() {
        if let Some(avg_min_max) = avg_min_max(val) {
            println!(
                "{}: {} {} {}  ({} runs)",
                key,
                avg_min_max.avg.yellow().whenever(Condition::TTY_AND_COLOR),
                avg_min_max.min.green().whenever(Condition::TTY_AND_COLOR),
                avg_min_max.max.red().whenever(Condition::TTY_AND_COLOR),
                avg_min_max.number,
            );
        } else {
            println!("{}: _ _ _  (0 runs)", key);
        }
    }
}

/// The results of a run given by filter.name, Vec<duration>
/// Notice that not all vectors will have the same length as some runs might fail.
type RunResults<'a> = HashMap<&'a str, Vec<Duration>>;

/// Print the differences in computer format
fn print_computer(hash: RunResults) {
    for (key, items) in hash.iter() {
        print!("{key}: ");
        for i in items {
            print!("{}.{}, ", i.whole_seconds(), i.whole_microseconds())
        }
        println!();
    }
}

#[derive(Debug, Serialize)]
/// Struct for bencher json
struct Latency {
    #[serde(with = "rust_decimal::serde::float")]
    value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    lower_value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    upper_value: Decimal,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let filters = if let Some(ref path) = args.filter_file {
        filter::read_filter_file(path)?
    } else {
        vec![
            Filter {
                name: String::from("Surface->LoadStart"),
                first: Box::new(|t: &Trace| t.function.contains("on_surface_created_cb")),
                last: Box::new(|t: &Trace| t.function.contains("load status changed Head")),
            },
            Filter {
                name: String::from("Load->Compl"),
                first: Box::new(|t: &Trace| t.function.contains("load status changed Head")),
                last: Box::new(|t: &Trace| t.function.contains("PageLoadEndedPrompt")),
            },
        ]
    };

    if !device::is_device_reachable().context("Testing reachability of device")? {
        return Err(anyhow!("No phone seems to be reachable"));
    }

    ctrlc::set_handler(move || {
        device::stop_tracing(args.trace_buffer).expect("Could not stop tracing");
    })?;

    let mut results: HashMap<&str, Vec<Duration>> = HashMap::new();
    let mut errors: HashMap<&str, u32> = HashMap::new();
    for i in 1..args.tries + 1 {
        if !args.bencher {
            println!("Running test {}", i);
        }
        let log_path = device::exec_hdc_commands(&args)?;
        let traces = device::read_file(&log_path)?;
        let differences = filter::find_notable_differences(&traces, &filters);
        for (key, value) in differences.iter() {
            if let Ok(d) = value {
                results
                    .entry(key)
                    .and_modify(|v| v.push(*d))
                    .or_insert(vec![(*d)]);
            } else {
                errors.entry(key).and_modify(|v| *v += 1).or_insert(1);
            }
        }

        if args.tries == 1 && args.all_traces {
            println!("Printing {} traces", &traces.len());
            for i in &traces {
                println!("{:?}", i);
            }
            println!("----------------------------------------------------------\n\n");
        }
    }

    if args.computer_output {
        print_computer(results);
    } else if args.bencher {
        bencher::write_results(&args, results);
    } else {
        print_differences(&args, results, errors);
    }

    Ok(())
}
