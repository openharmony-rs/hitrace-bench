use std::sync::LazyLock;

use itertools::Itertools;
use log::error;
use regex::{Captures, Regex};
use serde::Deserialize;

use crate::{
    runconfig::RunConfig,
    trace::{Trace, TraceMarker},
};

const SERVO_MEMORY_PROFILING_STRING: &str = "servo_memory_profiling";
const SERVO_LCP_STRING: &str = "LargestContentfulPaint";
const SERVO_FCP_STRING: &str = "FirstContentfulPaint";

// checked Default, Deserialize^,
#[derive(Debug, Deserialize, Default, PartialEq)]
pub(crate) enum PointFilterType {
    #[default]
    Default,
    Combined,
    Largest,
}

/// We have different type of points which have different regexp.
/// See the statics for a detailed explanation
pub(crate) enum PointType {
    /// A memory report that has an url attached, like LayoutThread.
    MemoryUrl(u64),
    /// A simple memory report, corresponding to resident-memory most likely.
    MemoryReport(u64),
    /// Report of smaps.
    Smaps(u64),
    /// Report of a testcase point.
    Testcase(u64),
    /// A testcase point that will be the sum of all matches points.
    Combined(u64),
    /// LCP
    LargestContentfulPaint(u64),
}

impl PointType {
    pub fn numeric_value(&self) -> Option<u64> {
        match self {
            PointType::MemoryUrl(v)
            | PointType::MemoryReport(v)
            | PointType::Smaps(v)
            | PointType::Testcase(v)
            | PointType::Combined(v)
            | PointType::LargestContentfulPaint(v) => Some(*v),
        }
    }
}

/// Notice that this also matches MEMORY_URL_REPORT
/// This is the general regexp
/// Example: servo_memory_profiling:resident 270778368
static MEMORY_REPORT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^servo_memory_profiling:(.*?)\s(\d+)$",
        "|",
        r"^servo_memory_profiling:(.*?)\|(\d+)\|\w*$"
    ))
    .expect("Could not parse regexp")
});

/// Reports that contain an url/iframe
/// Example: servo_memory_profiling:url(https://servo.org/)/js/non-heap 262144
static MEMORY_URL_REPORT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^servo_memory_profiling:url\((.*?)\)\/(.*?)\s(\d+)$",
        "|",
        r"^servo_memory_profiling:url\((.*?)\)\/(.*?)\|(\d+)\|\w*$"
    ))
    .expect("Could not parse regexp")
});

/// resident-according-to-smaps has again a different way
/// Example: servo_memory_profiling:resident-according-to-smaps/other 60424192
static SMAPS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^servo_memory_profiling:(resident-according-to-smaps)\/(.*)\s(\d+)$",
        "|",
        r"^servo_memory_profiling:(resident-according-to-smaps)\/(.*)\|(\d+)\|\w*$"
    ))
    .expect("Could not parse regexp")
});

static TESTCASE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^TESTCASE_PROFILING: (.*?) (\d+)$",
        "|",
        r"^TESTCASE_PROFILING: (.*?)\|(\d+)\|\w*$"
    ))
    .expect("Could not parse regexp")
});

static LCP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(LargestContentfulPaint)\|\w*\|(.*?)$").expect("Could not parse regexp")
});

static FCP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(FirstContentfulPaint)\|\w*\|(.*?)$").expect("Could not parse regexp")
});

/// This regex is to parse LCP and FCP key=value pair field
///
/// LCP case: paint_time=CrossProcessInstant { value: 231277222481376 },area=4095,lcp_type=Image,pipeline_id=(1,1)
/// FCP case: epoch=Epoch(1),paint_time=CrossProcessInstant { value: 271633800350218 },pipeline_id=(1,1)
static CROSS_PROCESS_INSTANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:epoch=Epoch\(\d*\),)?paint_time=CrossProcessInstant\s*\{\s*value:\s*(\d+)\s*\},(?:area=(\d*).*$)?")
        .expect("Could not parse regexp")
});

/// A parsed trace point metric
pub(crate) struct Point<'a> {
    /// The name you gave to this point
    pub(crate) name: String,
    /// Do not convert units
    pub(crate) no_unit_conversion: bool,
    /// The type of point this matches to
    pub(crate) point_type: PointType,
    /// The trace this matches to
    pub(crate) trace: Option<&'a Trace>,
}

/// You might want to extract data points. These do not have a beginning and end, just a point.
#[derive(Debug, Deserialize)]
pub(crate) struct PointFilter {
    /// The name we will use for this string
    pub(crate) name: String,
    /// We substring match on this
    pub(crate) match_str: String,
    /// Should we not assume this is in kb?
    #[serde(default)]
    pub(crate) no_unit_conversion: bool,
    /// This is more flexible version of "combined", but did not replace it fully due to input json
    #[serde(default)]
    pub(crate) point_filter_type: PointFilterType,
}

impl PointFilter {
    pub(crate) fn new(name: String, match_str: String) -> Self {
        PointFilter {
            name,
            match_str,
            no_unit_conversion: false,
            point_filter_type: PointFilterType::Default,
        }
    }

    /// This filters sub memory reports with a url attached.
    fn filter_memory_url<'a>(
        &'a self,
        run_config: &RunConfig,
        groups: Captures,
        trace: &'a Trace,
    ) -> Option<Point<'a>> {
        let mut match_iter = groups.iter().flatten();
        let _whole_match = match_iter.next();
        let url = match_iter.next().expect("No match").as_str();
        let subsystem_path = match_iter.next().expect("No match").as_str();
        let value = match_iter
            .next()
            .expect("No match")
            .as_str()
            .parse()
            .expect("Could not parse value");
        if url.contains(run_config.run_args.url.as_str()) {
            let mut suffix = subsystem_path.split('/').skip(1).join("/");
            if !suffix.is_empty() {
                suffix.insert(0, '/');
            }
            Some(Point {
                name: run_config.run_args.url.to_owned()
                    + "/"
                    + self.name.as_str()
                    + suffix.as_str(),
                no_unit_conversion: self.no_unit_conversion,
                trace: Some(trace),
                point_type: PointType::MemoryUrl(value),
            })
        } else {
            None
        }
    }

    /// This filters smaps reports
    fn filter_smaps<'a>(
        &'a self,
        run_config: &RunConfig,
        groups: Captures,
        trace: &'a Trace,
    ) -> Option<Point<'a>> {
        let mut match_iter = groups.iter().flatten();
        let _whole_match = match_iter.next();
        let match_str = match_iter.next().unwrap().as_str();
        let _fn_name = match_iter.next();
        if match_str != self.match_str {
            None
        } else {
            let value = match_iter
                .next()
                .expect("Could not find match")
                .as_str()
                .parse()
                .expect("Could not parse");
            Some(Point {
                name: run_config.run_args.url.to_owned() + "/" + self.name.as_str(),
                no_unit_conversion: self.no_unit_conversion,
                trace: Some(trace),
                point_type: PointType::Smaps(value),
            })
        }
    }

    /// This simple memory reports
    fn filter_memory<'a>(
        &'a self,
        run_config: &RunConfig,
        groups: Captures,
        trace: &'a Trace,
    ) -> Option<Point<'a>> {
        let mut match_iter = groups.iter().flatten();
        let _whole_match = match_iter.next();
        let _name = match_iter.next();

        let value = match_iter
            .next()
            .expect("Could not find match")
            .as_str()
            .parse()
            .expect("Could not parse value");
        Some(Point {
            name: run_config.run_args.url.to_owned() + "/" + self.name.as_str(),
            no_unit_conversion: self.no_unit_conversion,
            trace: Some(trace),
            point_type: PointType::MemoryReport(value),
        })
    }

    /// This filters test cases
    fn filter_testcase<'a>(
        &'a self,
        run_config: &RunConfig,
        groups: Captures,
        trace: &'a Trace,
    ) -> Option<Point<'a>> {
        let mut match_iter = groups.iter().flatten();
        let _whole_match = match_iter.next();
        let name = match_iter.next();

        let case_name = name.expect("Could not find match").as_str();
        let value = match_iter
            .next()
            .expect("Could not find match")
            .as_str()
            .parse()
            .expect("Could not parse value");
        if case_name.contains(&self.match_str) {
            Some(Point {
                name: run_config.run_args.url.to_owned() + "/",
                no_unit_conversion: self.no_unit_conversion,
                trace: Some(trace),
                point_type: PointType::Testcase(value),
            })
        } else {
            None
        }
    }

    /// This filters LCP cases
    fn filter_lcp_or_fcp<'a>(
        &'a self,
        run_config: &RunConfig,
        groups: Captures,
        trace: &'a Trace,
    ) -> Option<Vec<Point<'a>>> {
        let mut match_iter = groups.iter().flatten();
        let _whole_match = match_iter.next();
        let filter_name = match_iter.next().expect("Could not find match").as_str();
        let key_values = match_iter.next().expect("Could not find match").as_str();

        if filter_name == SERVO_LCP_STRING {
            let lcp_values = parse_lcp_trace(key_values).expect("Could not parse LCP values");
            Some(vec![
                Point {
                    name: run_config.run_args.url.to_owned()
                        + "/"
                        + self.name.as_str()
                        + "/paint_time",
                    no_unit_conversion: self.no_unit_conversion,
                    trace: Some(trace),
                    point_type: PointType::LargestContentfulPaint(lcp_values.paint_time),
                },
                Point {
                    name: run_config.run_args.url.to_owned() + "/" + self.name.as_str() + "/area",
                    no_unit_conversion: self.no_unit_conversion,
                    trace: Some(trace),
                    point_type: PointType::LargestContentfulPaint(lcp_values.area),
                },
            ])
        } else if filter_name == SERVO_FCP_STRING {
            Some(vec![Point {
                name: run_config.run_args.url.to_owned() + "/" + self.name.as_str() + "/paint_time",
                no_unit_conversion: self.no_unit_conversion,
                trace: Some(trace),
                point_type: PointType::LargestContentfulPaint(
                    parse_fcp_trace(key_values)
                        .expect("Could not parse LCP values")
                        .paint_time,
                ),
            }])
        } else {
            None
        }
    }

    /// This is the main filter function which will call the corresponding filter functions
    fn filter_trace_to_option_point<'a>(
        &'a self,
        trace: &'a Trace,
        run_config: &RunConfig,
    ) -> Option<Vec<Point<'a>>> {
        if let Some(groups) = LCP_REGEX.captures(&trace.function) {
            self.filter_lcp_or_fcp(run_config, groups, trace)
        } else if let Some(groups) = FCP_REGEX.captures(&trace.function) {
            self.filter_lcp_or_fcp(run_config, groups, trace)
        } else {
            if let Some(groups) = MEMORY_URL_REPORT_REGEX.captures(&trace.function) {
                self.filter_memory_url(run_config, groups, trace)
            } else if let Some(groups) = SMAPS_REGEX.captures(&trace.function) {
                self.filter_smaps(run_config, groups, trace)
            } else if let Some(groups) = MEMORY_REPORT_REGEX.captures(&trace.function) {
                self.filter_memory(run_config, groups, trace)
            } else if let Some(groups) = TESTCASE_REGEX.captures(&trace.function) {
                self.filter_testcase(run_config, groups, trace)
            } else {
                None
            }
            .map(|p| vec![p])
        }
    }

    /// Check if there are duplicates for PointType::Testcase and PointType::MemoryReport.
    /// Remove these and print errors.
    fn remove_duplicates(&self, points: &mut Vec<Point>) {
        if points
            .iter()
            .filter(|p| matches!(p.point_type, PointType::Testcase(_)))
            .count()
            > 1
        {
            error!(
                "PointFilter {:?} matched with multiple traces {:?}. Discarding",
                self,
                points
                    .iter()
                    .filter_map(|p| p.trace)
                    .collect::<Vec<&Trace>>()
            );
            points.retain(|p| !matches!(p.point_type, PointType::Testcase(_)));
        }

        if points
            .iter()
            .filter(|p| matches!(p.point_type, PointType::MemoryReport(_)))
            .count()
            > 1
        {
            error!(
                "PointFilter {:?} matched with multiple traces {:?}. Discarding",
                self,
                points
                    .iter()
                    .filter_map(|p| p.trace)
                    .collect::<Vec<&Trace>>()
            );
            points.retain(|p| !matches!(p.point_type, PointType::MemoryReport(_)));
        }
    }

    /// Takes a a `PointFilter`, an array of traces and a run_config to create a result of matched points.
    pub(crate) fn pointfilter_to_point<'a>(
        &'a self,
        traces: &'a [Trace],
        run_config: &'a RunConfig,
    ) -> Vec<Point<'a>> {
        let mut points: Vec<_> = traces
            .iter()
            .filter(|t| {
                t.trace_marker == TraceMarker::Dot || t.trace_marker == TraceMarker::StartSync
            })
            .filter(|t| {
                t.function.contains(SERVO_MEMORY_PROFILING_STRING)
                    || t.function.contains("TESTCASE_PROFILING")
                    || t.function.contains(SERVO_LCP_STRING)
                    || t.function.contains(SERVO_FCP_STRING)
            })
            .filter(|t| t.function.contains(&self.match_str))
            .filter_map(|t| self.filter_trace_to_option_point(t, run_config))
            .flatten()
            .collect();

        if !matches!(self.point_filter_type, PointFilterType::Default) {
            // we now need to collect points with the same name
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
                            no_unit_conversion: vals.first().unwrap().no_unit_conversion,
                            trace: None,
                            point_type: match self.point_filter_type {
                                PointFilterType::Largest => PointType::LargestContentfulPaint(
                                    vals.iter()
                                        .map(|p| p.point_type.numeric_value().unwrap())
                                        .max()
                                        .unwrap(),
                                ),

                                PointFilterType::Combined => PointType::Combined(
                                    vals.iter()
                                        .map(|p| p.point_type.numeric_value().unwrap())
                                        .sum(),
                                ),

                                PointFilterType::Default => panic!("should not be reachable"),
                            },
                        }
                    }
                })
                .collect()
        } else {
            self.remove_duplicates(&mut points);
            points
        }
    }
}

#[derive(PartialEq, Debug)]
struct LCPTraceValues {
    paint_time: u64,
    area: u64,
}
/// This function takes value from the hitrace-sys's start_trace_ex's `key=value,` string
///
/// Example paint_time=CrossProcessInstant { value: 219733332872200 },area=90810,pipeline_id=(1,1)
fn parse_lcp_trace(input: &str) -> Option<LCPTraceValues> {
    CROSS_PROCESS_INSTANT
        .captures(input)
        .map(|groups| LCPTraceValues {
            paint_time: groups
                .get(1)
                .expect("Could not find paint_time in LCP trace using REGEX")
                .as_str()
                .parse()
                .expect("Count not parse paint_time from LCP trace using REGEX"),
            area: groups
                .get(2)
                .expect("Could not find paint_time in LCP trace using REGEX")
                .as_str()
                .parse()
                .expect("Count not parse paint_time from LCP trace using REGEX"),
        })
}

#[test]
fn test_trace_kv_parsing() {
    let test_str =
        "paint_time=CrossProcessInstant { value: 231277222481376 },area=4095,lcp_type=Image,pipeline_id=(1,1)"
            .to_string();

    assert_eq!(
        parse_lcp_trace(&test_str),
        Some(LCPTraceValues {
            paint_time: 231277222481376,
            area: 4095
        })
    );
}

#[derive(PartialEq, Debug)]
struct FCPTraceValue {
    paint_time: u64,
}

/// This function takes value from the hitrace-sys's start_trace_ex's `key=value,` string
///
/// Example "epoch=Epoch(1),paint_time=CrossProcessInstant { value: 271633800350218 },pipeline_id=(1,1)"
fn parse_fcp_trace(input: &str) -> Option<FCPTraceValue> {
    if let Some(groups) = CROSS_PROCESS_INSTANT.captures(input) {
        return Some(FCPTraceValue {
            paint_time: groups
                .get(1)
                .expect("Could not find paint_time in LCP trace using REGEX")
                .as_str()
                .parse()
                .expect("Count not parse paint_time from LCP trace using REGEX"),
        });
    }
    None
}

#[test]
fn test_lcp_parsing() {
    assert_eq!(
        parse_lcp_trace(
            "paint_time=CrossProcessInstant { value: 231277222481376 },area=4095,lcp_type=Image,pipeline_id=(1,1)"
        ),
        Some(LCPTraceValues {
            paint_time: 231277222481376,
            area: 4095
        })
    );
}

#[test]
fn test_fcp_parsing() {
    assert_eq!(
        parse_fcp_trace(
            "epoch=Epoch(1),paint_time=CrossProcessInstant { value: 271633800350218 },pipeline_id=(1,1)"
        ),
        Some(FCPTraceValue {
            paint_time: 271633800350218,
        })
    );
}
