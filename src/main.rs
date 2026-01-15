use anyhow::{Context, Result, anyhow};
use args::Args;
use clap::Parser;
use filter::Filter;
use humanize_bytes::humanize_bytes_binary;
use log::{error, info};
use runconfig::RunConfig;
use std::collections::HashMap;
use time::Duration;
use trace::Trace;
use utils::{FilterErrors, FilterResults, PointResults, RunResults, avg_min_max};
use yansi::{Condition, Paint};

use crate::{args::RunArgs, point_filters::PointFilter, utils::PointResult};

mod args;
mod bencher;
mod device;
mod filter;
mod point_filters;
mod runconfig;
mod test;
mod trace;
mod utils;

/// Print the differences
fn print_differences(args: &RunArgs, results: RunResults) {
    if !results.errors.is_empty() {
        println!("The following things broke with errors");
        for (key, val) in results.errors.iter() {
            println!("{key}: {val} errors");
        }
    }

    println!(
        "\n----name {} {} {}------({}) runs (hp:{})------------------------",
        "avg".yellow(),
        "min".green(),
        "max".red(),
        args.tries,
        args.url
    );
    for (key, val) in results.filter_results.iter() {
        let avg_min_max = avg_min_max::<Duration, u16>(val);
        println!(
            "{}: {} {} {}  ({} runs)",
            key,
            avg_min_max.avg.yellow().whenever(Condition::TTY_AND_COLOR),
            avg_min_max.min.green().whenever(Condition::TTY_AND_COLOR),
            avg_min_max.max.red().whenever(Condition::TTY_AND_COLOR),
            avg_min_max.number,
        );
    }

    if !results.point_results.is_empty() {
        println!("-----------Points-------------------------");
        let mut sorted_points: Vec<_> = results.point_results.into_iter().collect();
        sorted_points.sort_by(|x, y| x.0.cmp(&y.0));
        for (key, val) in sorted_points {
            let avg_min_max = avg_min_max::<u64, u64>(&val.result);
            if val.no_unit_conversion {
                println!(
                    "{}: {} {} {} ({} runs)",
                    key,
                    avg_min_max.avg.yellow().whenever(Condition::TTY_AND_COLOR),
                    avg_min_max.min.green().whenever(Condition::TTY_AND_COLOR),
                    avg_min_max.max.red().whenever(Condition::TTY_AND_COLOR),
                    avg_min_max.number
                );
            } else {
                println!(
                    "{}: {} {} {}  ({} runs)",
                    key,
                    humanize_bytes_binary!(avg_min_max.avg)
                        .yellow()
                        .whenever(Condition::TTY_AND_COLOR),
                    humanize_bytes_binary!(avg_min_max.min)
                        .green()
                        .whenever(Condition::TTY_AND_COLOR),
                    humanize_bytes_binary!(avg_min_max.max)
                        .red()
                        .whenever(Condition::TTY_AND_COLOR),
                    avg_min_max.number,
                );
            }
        }
        println!();
    }
}

/// Process the filters from traces. These are the traces per run_config
fn run_runconfig_filters(
    run_config: &RunConfig,
    traces: &[Trace],
    results: &mut FilterResults,
    errors: &mut FilterErrors,
) {
    // Collect differences
    let differences = filter::find_notable_differences(traces, &run_config.filters);
    for (original_key, value) in differences.into_iter() {
        let key = if run_config.args.run_file.is_some() {
            format!("{}/{}", run_config.run_args.url, original_key)
        } else {
            original_key.to_owned()
        };
        if let Ok(d) = value {
            results
                .entry(key)
                .and_modify(|v| v.push(d))
                .or_insert(vec![(d)]);
        } else {
            errors.entry(key).and_modify(|v| *v += 1).or_insert(1);
        }
    }
}

/// Process the points from thre traces. These are the traces per run_config.
fn run_runconfig_points(run_config: &RunConfig, traces: &[Trace], points: &mut PointResults) {
    let new_points: Vec<_> = run_config
        .point_filters
        .iter()
        .map(|f| f.pointfilter_to_point(traces, run_config))
        .collect();

    for p in new_points.into_iter().flatten() {
        let key = p.name.to_owned();
        points
            .entry(key)
            .and_modify(|v| v.result.push(p.value))
            .or_insert(PointResult {
                no_unit_conversion: p.no_unit_conversion,
                result: vec![p.value],
            });
    }
}

/// Runs one RunConfig and append the results to the results, errors and points
pub(crate) fn run_runconfig(
    run_config: &RunConfig,
    results: &mut FilterResults,
    filter_errors: &mut FilterErrors,
    points: &mut PointResults,
) -> Result<()> {
    for i in 1..run_config.run_args.tries + 1 {
        info!("Running test {i}");
        let traces = if let Some(ref file) = run_config.args.trace_file {
            trace::read_file(file)?
        } else {
            let log_path =
                device::exec_hdc_commands(&run_config.run_args, run_config.args.is_rooted)?;
            trace::read_file(&log_path)?
        };
        run_runconfig_filters(run_config, &traces, results, filter_errors);
        run_runconfig_points(run_config, &traces, points);

        if run_config.run_args.tries == 1 && run_config.run_args.all_traces {
            println!("Printing {} traces", &traces.len());
            for i in &traces {
                println!("{i:?}");
            }
            println!("----------------------------------------------------------\n\n");
        }
    }
    Ok(())
}

/// Runs runconfigs
/// Bencher has to be treated separately because it wants a valid json output.
fn run_runconfigs(args: &Args, run_configs: &Vec<RunConfig>, use_bencher: bool) -> Result<()> {
    info!("Running with Args {args:?}");

    // bencher needs all runs, while a normal output can have the runs one after the other
    if use_bencher {
        let mut filter_results = HashMap::new();
        let mut filter_errors = HashMap::new();
        let mut point_results = HashMap::new();
        for run_config in run_configs {
            run_runconfig(
                run_config,
                &mut filter_results,
                &mut filter_errors,
                &mut point_results,
            )?;
        }

        bencher::write_results(RunResults {
            prepend: args.prepend.clone(),
            filter_results,
            errors: filter_errors,
            point_results,
        })
        .context("Error in writing bencher results")?
    } else {
        for run_config in run_configs {
            let mut filter_results = HashMap::new();
            let mut errors = HashMap::new();
            let mut point_results = HashMap::new();
            run_runconfig(
                run_config,
                &mut filter_results,
                &mut errors,
                &mut point_results,
            )?;
            print_differences(
                &run_config.run_args,
                RunResults {
                    prepend: args.prepend.clone(),
                    filter_results,
                    errors,
                    point_results,
                },
            );
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let run_configs = {
        if let Some(ref file) = args.run_file {
            runconfig::read_run_file(file, &args)?
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
            let point_filters = vec![
                PointFilter {
                    name: String::from("Explicit"),
                    match_str: String::from("explicit"),
                    no_unit_conversion: false,
                    combined: false,
                },
                PointFilter::new(String::from("Resident"), String::from("resident")),
                PointFilter::new(String::from("LayoutThread"), String::from("layout-thread")),
                PointFilter::new(String::from("image-cache"), String::from("image-cache")),
                PointFilter::new(String::from("JS"), String::from("js")),
                PointFilter {
                    name: String::from("resident-smaps"),
                    match_str: String::from("resident-according-to-smaps"),
                    no_unit_conversion: false,
                    combined: true,
                },
            ];

            vec![RunConfig::new(
                args.clone(),
                RunArgs::default(),
                filters,
                point_filters,
            )]
        }
    };

    if !device::is_device_reachable().context("Testing reachability of device")? {
        return Err(anyhow!("No phone seems to be reachable"));
    }

    let trace_buffer = run_configs
        .first()
        .expect("Need at least one RunConfig")
        .run_args
        .trace_buffer;

    let all_bencher = run_configs.iter().all(|r| r.args.bencher);
    let all_print = run_configs.iter().all(|r| !r.args.bencher);
    if !all_bencher && !all_print {
        error!("We only support all bencher or all print runs");
        return Ok(());
    }
    let be_loud_filter = if args.quiet || all_bencher {
        log::LevelFilter::Error
    } else {
        log::LevelFilter::Info
    };

    env_logger::builder().filter_level(be_loud_filter).init();

    ctrlc::set_handler(move || {
        device::stop_tracing(trace_buffer).expect("Could not stop tracing");
    })?;

    run_runconfigs(&args, &run_configs, all_bencher)?;

    Ok(())
}
