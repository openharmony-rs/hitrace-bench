use anyhow::{Context, Result, anyhow};
use itertools::Itertools;
use regex::{Captures, Regex};
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf, sync::LazyLock};
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
    /// With this we combine all points that match a substring
    #[serde(default)]
    pub(crate) combined: bool,
}

/// Notice that this also matches MEMORY_URL_REPORT
/// This is the general regexp
/// Example: servo_memory_profiling:resident 270778368
static MEMORY_REPORT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^servo_memory_profiling:(.*?)\s(\d+)$").expect("Could not parse regexp")
});

/// Reports that contain an url/iframe
/// Example: servo_memory_profiling:url(https://servo.org/)/js/non-heap 262144
static MEMORY_URL_REPORT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^servo_memory_profiling:url\((.*?)\)\/(.*?)\s(\d+)$")
        .expect("Could not parse regexp")
});

/// resident-according-to-smaps has again a different way
/// Example: servo_memory_profiling:resident-according-to-smaps/other 60424192
static SMAPS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^servo_memory_profiling:(resident-according-to-smaps)\/(.*)\s(\d+)$")
        .expect("Could not parse regexp")
});

impl PointFilter {
    pub(crate) fn new(name: String, match_str: String) -> Self {
        PointFilter {
            name,
            match_str,
            no_unit_conversion: false,
            combined: false,
        }
    }

    fn filter_memory_url(&self, run_config: &RunConfig, groups: Captures) -> Option<Point> {
        let url = groups.get(1).expect("No match").as_str();
        let fn_name = groups.get(2).expect("No match").as_str();
        let value = groups
            .get(3)
            .expect("No match")
            .as_str()
            .parse()
            .expect("Could not parse value");
        if url.contains(run_config.args.url.as_str()) {
            let mut suffix = fn_name.split('/').skip(1).join("/");
            if !suffix.is_empty() {
                suffix.insert(0, '/');
            }
            Some(Point {
                name: self.name.clone() + suffix.as_str(),
                value,
                no_unit_conversion: self.no_unit_conversion,
            })
        } else {
            None
        }
    }

    fn filter_smaps(&self, groups: Captures) -> Option<Point> {
        if groups.get(1).unwrap().as_str() != self.match_str {
            None
        } else {
            let value = groups
                .get(3)
                .unwrap()
                .as_str()
                .parse()
                .expect("Could not parse");
            Some(Point {
                name: self.name.clone(),
                value,
                no_unit_conversion: self.no_unit_conversion,
            })
        }
    }

    fn filter_memory(&self, groups: Captures) -> Option<Point> {
        // this regex also matches the above characters
        //t fn_name = groups.get(1).expect("No match").as_str();
        let value = groups
            .get(2)
            .expect("Could not find match")
            .as_str()
            .parse()
            .expect("Could not parse value");
        //if fn_name != self.match_str {
        //            None
        //        } else {
        Some(Point {
            name: self.name.clone(),
            value,
            no_unit_conversion: self.no_unit_conversion,
        })
        //      }
    }

    /// this is the main filter function
    fn filter_trace_to_option_point(&self, trace: &Trace, run_config: &RunConfig) -> Option<Point> {
        if let Some(groups) = MEMORY_URL_REPORT_REGEX.captures(&trace.function) {
            self.filter_memory_url(run_config, groups)
        } else if let Some(groups) = SMAPS_REGEX.captures(&trace.function) {
            println!("trace matches smaps {:?} {:?}", &trace.function, self.name);
            self.filter_smaps(groups)
        } else if let Some(groups) = MEMORY_REPORT_REGEX.captures(&trace.function) {
            self.filter_memory(groups)
        } else {
            None
        }
    }

    pub(crate) fn pointfilter_to_point(
        &self,
        traces: &[Trace],
        run_config: &RunConfig,
    ) -> Vec<Point> {
        let points: Vec<_> = traces
            .iter()
            .filter(|t| t.trace_marker == TraceMarker::Dot)
            .filter(|t| {
                t.function.contains(SERVO_MEMORY_PROFILING_STRING)
                    || t.function.contains("TESTCASE_PROFILING")
            })
            .filter(|t| t.function.contains(&self.match_str))
            .filter_map(|t| self.filter_trace_to_option_point(t, run_config))
            .collect();

        if self.combined {
            // we now need to collect points with the same name
            //println!("points {:?}\n\n", points);
            points
                .into_iter()
                .into_group_map_by(|p| p.name.clone())
                .into_iter()
                .map(|(name, mut vals)| {
                    if vals.len() == 1 {
                        vals.remove(0)
                    } else {
                        Point {
                            name,
                            value: vals.iter().map(|p| p.value).sum(),
                            no_unit_conversion: vals.first().unwrap().no_unit_conversion,
                        }
                    }
                })
                .collect()
        } else {
            println!("popints {:?}\n\n", points);
            points
        }
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
