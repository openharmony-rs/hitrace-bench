/// Functions about the traces
use anyhow::{Result, anyhow};
use std::fmt::{Debug, Display, write};
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

#[derive(Clone, Debug)]
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
