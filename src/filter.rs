use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf};
use time::Duration;

use crate::{Trace, trace::difference_of_traces};

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

#[derive(Deserialize)]
/// The json type to filter
struct JsonFilterDescription {
    /// The name the filter should have
    name: String,
    /// We will match the start of the filter to contain this function name
    start_fn_partial: String,
    /// We will match the end of the filter to contain this function name
    end_fn_partial: String,
}

impl From<JsonFilterDescription> for Filter {
    fn from(value: JsonFilterDescription) -> Self {
        Filter {
            name: value.name,
            first: Box::new(move |trace: &Trace| trace.function.contains(&value.start_fn_partial)),
            last: Box::new(move |trace: &Trace| trace.function.contains(&value.end_fn_partial)),
        }
    }
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
