/// Functions about the traces
use anyhow::{Context, Result, anyhow};
use log::error;
use regex::Regex;
use std::{
    fmt::{Debug, Display, write},
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};
use time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct TimeStamp {
    pub(crate) seconds: u64,
    pub(crate) micro: u64,
}

impl Display for TimeStamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write(f, format_args!("{}.{:6}", self.seconds, self.micro))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TraceMarker {
    StartSync,
    EndSync,
    StartAsync,
    EndAsync,
    Dot,
}

impl TraceMarker {
    pub(crate) fn from(val: &str) -> Result<Self> {
        match val {
            "B" => Ok(TraceMarker::StartSync),
            "E" => Ok(TraceMarker::EndSync),
            "S" => Ok(TraceMarker::StartAsync),
            "F" => Ok(TraceMarker::EndAsync),
            "C" => Ok(TraceMarker::Dot),
            _ => Err(anyhow!("Could not parse Trace Marker")),
        }
    }
}

#[derive(Clone)]
/// A parsed trace
pub(crate) struct Trace {
    /// Name of the thread, i.e., `org.servo.servo`` or `Constellation`
    #[allow(unused)]
    pub(crate) name: String,
    /// pid
    #[allow(unused)]
    pub(crate) pid: u64,
    /// the cpu it ran on
    #[allow(unused)]
    pub(crate) cpu: u64,
    /// timestamp of the trace
    pub(crate) timestamp: TimeStamp,
    /// Tells us if the trace ended and when
    #[allow(unused)]
    pub(crate) trace_marker: TraceMarker,
    /// No idea what this is
    #[allow(unused)]
    pub(crate) number: String,
    /// Some shorthand code
    #[allow(unused)]
    pub(crate) shorthand: String,
    /// Full function name
    pub(crate) function: String,
}

impl Debug for Trace {
    /// We roughly want this output but shorted
    /// org.servo.servo-44962   (  44682) [010] .... 17864.716645: tracing_mark_write: B|44682|ML: do_single_part3_compilation`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Trace: {}-{} ... {}.{}: {}",
            self.name, self.pid, self.timestamp.seconds, self.timestamp.micro, self.function,
        )
    }
}

#[cfg(test)]
impl Trace {
    pub(crate) fn new(
        pid: u64,
        timestamp_secs: u64,
        trace_marker: TraceMarker,
        function: &str,
    ) -> Self {
        Trace {
            name: "Test".to_owned(),
            pid,
            cpu: 1,
            timestamp: TimeStamp {
                seconds: timestamp_secs,
                micro: 0,
            },
            trace_marker,
            number: String::from("1"),
            shorthand: String::from("1"),
            function: function.to_owned(),
        }
    }
}

/// Calculates the timestamp difference equaivalent to trace1-trace2
pub(crate) fn difference_of_traces(trace1: &Trace, trace2: &Trace) -> Duration {
    Duration::new(
        trace1.timestamp.seconds as i64 - trace2.timestamp.seconds as i64,
        (trace1.timestamp.micro as i32 - trace2.timestamp.micro as i32) * 1000,
    )
}

/// There is always one trace per line
/// This means that having no matched lines is ok and returns None. Having a parsing error returns Some(Err)
fn line_to_trace(regex: &Regex, line: &str) -> Option<Result<Trace>> {
    regex
        .captures_iter(line)
        .map(|c| c.extract())
        .map(match_to_trace)
        .next()
}

/// Read a regex matched line into a trace
fn match_to_trace(
    (
        _line,
        [
            name,
            pid,
            cpu,
            time1,
            time2,
            trace_marker,
            number,
            shorthand,
            msg,
        ],
    ): (&str, [&str; 9]),
) -> Result<Trace> {
    let seconds = time1.parse()?;
    let microseconds = time2.parse()?;
    let timestamp = TimeStamp {
        seconds,
        micro: microseconds,
    };
    let trace_marker = TraceMarker::from(trace_marker)?;
    Ok(Trace {
        name: name.to_owned(),
        pid: pid.parse().unwrap(),
        cpu: cpu.parse().unwrap(),
        trace_marker,
        number: number.to_string(),
        timestamp,
        shorthand: shorthand.to_owned(),
        function: msg.to_owned(),
    })
}

/// Read a file into traces
pub(crate) fn read_file(f: &Path) -> Result<Vec<Trace>> {
    // This is more specific servo tracing with the tracing_mark_write
    // Example trace: ` org.servo.servo-44962   (  44682) [010] .... 17864.716645: tracing_mark_write: B|44682|ML: do_single_part3_compilation`
    let regex = Regex::new(
        r"^\s*(.*?)\-(\d+)\s*\(\s*(\d+)\).*?(\d+)\.(\d+): tracing_mark_write: (.)\|(\d+?)\|(.*?):(.*)\s*$",
    ).expect("Could not read regex");
    let f = File::open(f).context("Could not find hitrace file")?;
    let reader = BufReader::new(f);

    let (valid_lines, invalid_lines): (Vec<_>, Vec<_>) = reader
        .lines()
        .enumerate()
        .partition(|(_index, l)| l.is_ok());

    if !invalid_lines.is_empty() {
        error!(
            "Could not read lines {:?}",
            invalid_lines
                .iter()
                .map(|(index, _l)| index)
                .collect::<Vec<_>>()
        );
    }

    valid_lines
        .into_iter()
        .filter_map(|(_index, l)| line_to_trace(&regex, &l.unwrap()))
        .collect::<Result<Vec<Trace>>>()
        .context("Could not parse one thing")
}
