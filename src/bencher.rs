use std::{collections::HashMap, fs::File, io::BufWriter};

use anyhow::Context;
use rust_decimal::Decimal;
use serde::Serialize;
use time::Duration;

use crate::{avg_min_max, utils::RunResults};

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

/// Converts duration to bencher Decimal representation. Duration has precision of nanoseconds
fn difference_to_bencher_decimal(dur: &Duration) -> Decimal {
    let number = dur.whole_nanoseconds();
    Decimal::from_i128_with_scale(number, 0)
}

type BencherLatency<'a> = HashMap<&'a str, Latency>;
#[derive(Serialize)]
#[serde(untagged)]
enum Bencher<'a> {
    Latency(BencherLatency<'a>),
}

/// Creates a bencher key adding the E2E and prepend result
fn bencher_key(result: &RunResults, key: &str) -> String {
    if let Some(ref pre) = result.prepend {
        format!("{pre}/E2E/{key}")
    } else {
        format!("E2E/{key}")
    }
}

/// Creates an iterator for the filter results with the appropriate map
fn filter_iterator(result: &RunResults) -> impl std::iter::Iterator<Item = (String, Bencher<'_>)> {
    result.filter_results.iter().map(|(key, dur_vec)| {
        let avg_min_max = avg_min_max::<Duration, u16>(dur_vec);
        // yes we need this hashmap for the correct json
        let mut map = HashMap::new();
        map.insert(
            "Latency",
            Latency {
                value: difference_to_bencher_decimal(&avg_min_max.avg),
                lower_value: difference_to_bencher_decimal(&avg_min_max.min),
                upper_value: difference_to_bencher_decimal(&avg_min_max.max),
            },
        );
        (bencher_key(result, key), Bencher::Latency(map))
    })
}

/// Creates an iterator for the point results with the appropriate map
fn points_iterator(result: &RunResults) -> impl std::iter::Iterator<Item = (String, Bencher<'_>)> {
    result.point_results.iter().map(|(key, points)| {
        let mut name = if points.no_unit_conversion {
            "Data"
        } else {
            "Memory"
        };
        if key.contains("LargestContentfulPaint/paint_time") {
            name = "Nanoseconds";
        }
        if key.contains("LargestContentfulPaint/area") {
            name = "Pixels";
        }
        let mut map = HashMap::new();
        let avg_min_max = avg_min_max::<u64, u64>(&points.result);
        map.insert(
            name,
            Latency {
                value: Decimal::from_i128_with_scale(avg_min_max.avg as i128, 0),
                lower_value: Decimal::from_i128_with_scale(avg_min_max.min as i128, 0),
                upper_value: Decimal::from_i128_with_scale(avg_min_max.max as i128, 0),
            },
        );
        (bencher_key(result, key), Bencher::Latency(map))
    })
}

/// Output in bencher json format to bench.json
/// We also will append it to the bench.json file instead of overwriting it so supsequent runs can be recorded.
/// We also add some custom strings to the filter.
pub(crate) fn write_results(result: RunResults) -> anyhow::Result<()> {
    let b = generate_results_hashmap(&result);

    let file = File::create("bench.json").context("Could not create bench.json file")?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &b).context("Could not serialize results")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&b).context("Could not serialize results")?
    );
    Ok(())
}

#[cfg(test)]
pub(crate) fn generate_result_json_str(result: RunResults) -> anyhow::Result<String> {
    let b = generate_results_hashmap(&result);
    serde_json::to_string_pretty(&b).context("Could not serialize results")
}

fn generate_results_hashmap<'a>(result: &'a RunResults) -> HashMap<String, Bencher<'a>> {
    let filters_iter = filter_iterator(result);
    let points_iter = points_iterator(result);

    // let b: HashMap<String, Bencher> = filters_iter.chain(points_iter).collect();
    filters_iter.chain(points_iter).collect()
}
