use anyhow::{Context, Result, anyhow};
use args::Args;
use clap::Parser;
use filter::Filter;
use runconfig::RunConfig;
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
mod runconfig;
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

/// runs a RunConfig
fn run_runconfig(run_config: &RunConfig) -> Result<()> {
    let mut results: HashMap<&str, Vec<Duration>> = HashMap::new();
    let mut errors: HashMap<&str, u32> = HashMap::new();

    for i in 1..run_config.args.tries + 1 {
        if !run_config.args.bencher {
            println!("Running test {}", i);
        }
        let log_path = device::exec_hdc_commands(&run_config.args)?;
        let traces = device::read_file(&log_path)?;
        let differences = filter::find_notable_differences(&traces, &run_config.filters);
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

        if run_config.args.tries == 1 && run_config.args.all_traces {
            println!("Printing {} traces", &traces.len());
            for i in &traces {
                println!("{:?}", i);
            }
            println!("----------------------------------------------------------\n\n");
        }
    }

    if run_config.args.bencher {
        bencher::write_results(&run_config.args, results)
    } else {
        print_differences(&run_config.args, results, errors)
    }
    Ok(())
}

fn main() -> Result<()> {
    let run_configs: Vec<RunConfig> = {
        let args = Args::parse();
        if let Some(file) = args.run_file {
            runconfig::read_run_file(&file)?
        } else if let Some(ref path) = args.filter_file {
            let filters = filter::read_filter_file(path)?;
            vec![RunConfig::new(args, filters)]
        } else {
            let filters = vec![
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
            ];
            vec![RunConfig::new(args, filters)]
        }
    };

    if !device::is_device_reachable().context("Testing reachability of device")? {
        return Err(anyhow!("No phone seems to be reachable"));
    }

    let trace_buffer = run_configs
        .first()
        .expect("Need at least one RunConfig")
        .args
        .trace_buffer;
    ctrlc::set_handler(move || {
        device::stop_tracing(trace_buffer).expect("Could not stop tracing");
    })?;

    for run_config in run_configs {
        run_runconfig(&run_config)?;
    }

    Ok(())
}
