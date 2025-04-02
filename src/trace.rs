use anyhow::{Result, anyhow};
/// Functions about the traces
use std::fmt::{Debug, Display, write};

#[derive(Debug)]
pub(crate) struct TimeStamp {
    pub(crate) seconds: u64,
    pub(crate) micro: u64,
}

impl Display for TimeStamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write(f, format_args!("{}.{:6}", self.seconds, self.micro))
    }
}

#[derive(Debug)]
pub(crate) enum TraceMarker {
    StartSync,
    EndSync,
    StartAsync,
    EndAsync,
    Dot,
}

impl TraceMarker {
    fn from(val: &str) -> Result<Self> {
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

#[derive(Debug)]
/// A parsed trace
pub(crate) struct Trace {
    /// Name of the program, i.e., org.servo.servo
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
    pub(crate) trace_marker: TraceMarker,
    /// No idea what this is
    #[allow(unused)]
    pub(crate) number: String,
    /// Some shorthand code
    pub(crate) shorthand: String,
    /// Full function name
    pub(crate) function: String,
}

/// Read a regex matched line into a trace
pub(crate) fn match_to_trace(
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
