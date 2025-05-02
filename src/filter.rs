use anyhow::{Result, anyhow};
use std::collections::HashMap;
use time::Duration;

use crate::{Trace, trace::difference_of_traces};

/// Way to construct filters
pub(crate) struct Filter<'a> {
    /// A name for the filter that will be output
    pub(crate) name: &'a str,
    /// A function taking a trace and deciding if it should be the start of the timing
    pub(crate) first: fn(&Trace) -> bool,
    /// A function taking a trace and deciding if it should be the end of the timing
    pub(crate) last: fn(&Trace) -> bool,
}

/// Turn a filter into a str and Result<Duration>
fn filter_to_duration<'a>(v: &[Trace], filter: &'a Filter) -> (&'a str, Result<Duration>) {
    let first = v
        .iter()
        .filter(|t| (filter.first)(t))
        .collect::<Vec<&Trace>>();
    let last = v
        .iter()
        .filter(|t| (filter.last)(t))
        .collect::<Vec<&Trace>>();

    let result = if first.len() != 1 || last.len() != 1 {
        Err(anyhow!(
            "Your filter functions are not specific or over specific, we got the following number of results: name: {}, first: {}, last: {}",
            filter.name,
            first.len(),
            last.len()
        ))
    } else {
        let first_trace = first.first().unwrap();
        let last_trace = last.first().unwrap();

        Ok(difference_of_traces(last_trace, first_trace))
    };

    (filter.name, result)
}

/// Look through the traces and find all timing differences coming from the filters
pub(crate) fn find_notable_differences<'a>(
    v: &[Trace],
    filters: &'a Vec<Filter>,
) -> HashMap<&'a str, Result<Duration>> {
    filters
        .iter()
        .map(|filter| filter_to_duration(v, filter))
        .collect()
}
