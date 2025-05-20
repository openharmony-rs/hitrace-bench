use std::{collections::HashMap, fs::OpenOptions, io::BufWriter};

use rust_decimal::Decimal;
use time::Duration;

use crate::{Latency, RunResults, avg_min_max};

/// Converts duration to bencher Decimal representation
fn difference_to_bencher_decimal(dur: &Duration) -> Decimal {
    let number = dur.whole_nanoseconds() as i64;
    Decimal::new(number, 3)
}

/// Output in bencher json format to bench.json
/// We also will append it to the bench.json file instead of overwriting it so supsequent runs can be recorded.
/// We also add some custom strings to the filter.
pub(crate) fn write_results(result: RunResults) {
    let b: HashMap<String, HashMap<&str, Latency>> = result
        .into_iter()
        .map(|(key, dur_vec)| {
            let avg_min_max = avg_min_max(&dur_vec);
            // yes we need this hashmap for the correct json
            let mut map = HashMap::new();
            if let Some(avg_min_max) = avg_min_max {
                map.insert(
                    "latency",
                    Latency {
                        value: difference_to_bencher_decimal(&avg_min_max.avg),
                        lower_value: difference_to_bencher_decimal(&avg_min_max.min),
                        upper_value: difference_to_bencher_decimal(&avg_min_max.max),
                    },
                );
            }
            (key, map)
        })
        .collect();

    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .open("bench.json")
        .expect("Could not open file");
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &b).expect("Could not write json");
    println!(
        "{}",
        serde_json::to_string_pretty(&b).expect("Could not serialize")
    );
}
