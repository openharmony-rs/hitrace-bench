use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf};
use time::Duration;

use crate::{
    RunConfig, Trace,
    runconfig::JsonFilterDescription,
    trace::{Point, TraceMarker, difference_of_traces},
};

const SERVO_MEMORY_PROFILING_STRING: &str = "servo_memory_profiling";

/// Way to construct filters
pub(crate) struct Filter {
    /// A name for the filter that will be output
    pub(crate) name: String,
    /// A function taking a trace and deciding if it should be the start of the timing
    pub(crate) first: Box<dyn Fn(&Trace) -> bool>,
    /// A function taking a trace and deciding if it should be the end of the timing
    pub(crate) last: Box<dyn Fn(&Trace) -> bool>,
}

impl Filter {
    /// Turn a filter into a str and Result<Duration>
    fn filter_to_duration(&self, v: &[Trace]) -> (&str, Result<Duration>) {
        let first = v
            .iter()
            .filter(|t| (self.first)(t))
            .collect::<Vec<&Trace>>();
        let last = v.iter().filter(|t| (self.last)(t)).collect::<Vec<&Trace>>();

        let result = if first.len() != 1 || last.len() != 1 {
            Err(anyhow!(
                "Your filter functions are not specific or over specific, we got the following number of results: name: {}, first: {}, last: {}",
                self.name,
                first.len(),
                last.len()
            ))
        } else {
            let first_trace = first.first().unwrap();
            let last_trace = last.first().unwrap();

            Ok(difference_of_traces(last_trace, first_trace))
        };

        (&self.name, result)
    }
}

/// You might want to extract data points. These do not have a beginning and end, just a point.
#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PointFilter {
    /// The name we will use for this string
    pub(crate) name: String,
    /// We substring match on this
    pub(crate) match_str: String,
    /// Should we not assume this is in kb?
    #[serde(default)]
    pub(crate) no_unit_conversion: bool,
}

impl PointFilter {
    pub(crate) fn new(name: String, match_str: String) -> Self {
        PointFilter {
            name,
            match_str,
            no_unit_conversion: false,
        }
    }

    /// this is the main filter function
    fn filter_trace_to_option_point(&self, trace: &Trace, run_config: &RunConfig) -> Option<Point> {
        let value_string = trace
            .function
            .split_whitespace()
            .last()
            .with_context(|| format!("Error in parsing trace {:?}", trace))
            .expect("Could not parse trace for last value");

        let value = value_string
            .parse()
            .with_context(|| format!("Error in parsing trace {:?}", trace))
            .expect("Could not parse number");

        if let Some(after_url) = trace.function.find(')') {
            // the url string will look like `servo_memory_profiling:(url)/` and we do not want the first /.
            // Additionally, different iframes will have different matches, so we do not want iframes that
            // are not part of the main url, so we return None and filter it.
            let (before_url_str, after_url_str) = trace.function.split_at(after_url + 2);
            if !before_url_str.contains(run_config.args.url.as_str()) {
                None
            } else {
                let name = self.name.clone()
                    + after_url_str
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .strip_prefix(&self.match_str)
                        .unwrap();
                Some(Point {
                    name,
                    value,
                    no_unit_conversion: self.no_unit_conversion,
                })
            }
        } else {
            Some(Point {
                name: self.name.clone(),
                value,
                no_unit_conversion: self.no_unit_conversion,
            })
        }
    }

    pub(crate) fn pointfilter_to_point(
        &self,
        traces: &[Trace],
        run_config: &RunConfig,
    ) -> Vec<Point> {
        traces
            .iter()
            .filter(|t| t.trace_marker == TraceMarker::Dot)
            .filter(|t| {
                t.function.contains(SERVO_MEMORY_PROFILING_STRING)
                    || t.function.contains("TESTCASE_PROFILING")
            })
            .filter(|t| t.function.contains(&self.match_str))
            .filter_map(|t| self.filter_trace_to_option_point(t, run_config))
            .collect()
    }
}

/// Look through the traces and find all timing differences coming from the filters
pub(crate) fn find_notable_differences<'a>(
    v: &[Trace],
    filters: &'a [Filter],
) -> HashMap<&'a str, Result<Duration>> {
    filters
        .iter()
        .map(|filter| filter.filter_to_duration(v))
        .collect()
}

pub(crate) fn read_filter_file(path: &PathBuf) -> Result<Vec<Filter>> {
    let file = File::open(path)
        .with_context(|| format!("Could not read filter file {}", path.to_string_lossy()))?;
    let reader = BufReader::new(file);
    let res: Vec<JsonFilterDescription> = serde_json::from_reader(reader)
        .context("Error in decoding filter file. Please look at the specification")?;
    Ok(res
        .into_iter()
        .map(|json_f| json_f.into())
        .collect::<Vec<Filter>>())
}
