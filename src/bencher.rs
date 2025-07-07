use std::{collections::HashMap, fs::File, io::BufWriter};

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

/// Output in bencher json format to bench.json
/// We also will append it to the bench.json file instead of overwriting it so supsequent runs can be recorded.
/// We also add some custom strings to the filter.
pub(crate) fn write_results(result: RunResults) {
    let filters_iter = result.filter_results.into_iter().map(|(key, dur_vec)| {
        let avg_min_max = avg_min_max::<Duration, u16>(&dur_vec);
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

        if let Some(ref pre) = result.prepend {
            (format!("{pre}/{key}"), Bencher::Latency(map))
        } else {
            (key, Bencher::Latency(map))
        }
    });

    let points_iter = result.point_results.into_iter().map(|(key, points)| {
        let name = if points.no_unit_conversion {
            "Data"
        } else {
            "Memory"
        };
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
        if let Some(ref pre) = result.prepend {
            (format!("{pre}/{key}"), Bencher::Latency(map))
        } else {
            (key, Bencher::Latency(map))
        }
    });

    let b: HashMap<String, Bencher> = filters_iter.chain(points_iter).collect();

    let file = File::create("bench.json").expect("Could not open file");
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &b).expect("Could not write json");
    println!(
        "{}",
        serde_json::to_string_pretty(&b).expect("Could not serialize")
    );
}
