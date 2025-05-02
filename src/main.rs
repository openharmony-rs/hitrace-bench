use anyhow::{Context, Result, anyhow};
use args::Args;
use clap::Parser;
use filter::Filter;
use rust_decimal::Decimal;
use serde::Serialize;
use std::{collections::HashMap, fs::File, io::BufWriter};
use time::Duration;
use trace::Trace;
use yansi::{Condition, Paint};

mod args;
mod device;
mod filter;
mod trace;

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
        args.homepage
    );
    for (key, val) in results.iter() {
        let avg = val
            .iter()
            .sum::<Duration>()
            .checked_div(args.tries as i32)
            .unwrap();
        let min = val.iter().min().unwrap();
        let max = val.iter().max().unwrap();
        println!(
            "{}: {} {} {}  ({} runs)",
            key,
            avg.yellow().whenever(Condition::TTY_AND_COLOR),
            min.green().whenever(Condition::TTY_AND_COLOR),
            max.red().whenever(Condition::TTY_AND_COLOR),
            val.iter().count(),
        );
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

#[derive(Serialize)]
/// Struct for bencher json
struct Latency {
    #[serde(with = "rust_decimal::serde::float")]
    value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    lower_value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    upper_value: Decimal,
}

/// Converts duration to bencher Decimal representation
fn difference_to_bencher_decimal(dur: &Duration) -> Decimal {
    let number = dur.whole_seconds() * 100 + dur.whole_milliseconds() as i64;
    Decimal::new(number, 3)
}

/// Output in bencher json format to bench.json
fn write_bencher(result: RunResults) {
    let b: HashMap<&str, HashMap<&str, Latency>> = result
        .into_iter()
        .map(|(key, dur_vec)| {
            let avg = dur_vec.iter().sum::<Duration>() / dur_vec.iter().count() as f64;
            let lower = dur_vec.iter().min();
            let upper = dur_vec.iter().max();

            // yes we need this hashmap for the correct json
            let mut map = HashMap::new();
            if lower.is_some() && upper.is_some() {
                map.insert(
                    "latency",
                    Latency {
                        value: difference_to_bencher_decimal(&avg),
                        lower_value: difference_to_bencher_decimal(lower.unwrap()),
                        upper_value: difference_to_bencher_decimal(upper.unwrap()),
                    },
                );
            }
            (key, map)
        })
        .collect();
    let file = File::create("bench.json").expect("Could not create file");
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &b).expect("Could not write json");
}

fn main() -> Result<()> {
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

    let args = Args::parse();

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
        let traces = device::read_file(&args, &log_path)?;
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
        write_bencher(results);
    } else {
        print_differences(&args, results, errors);
    }

    Ok(())
}
