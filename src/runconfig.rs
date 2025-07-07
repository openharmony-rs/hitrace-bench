use std::{fs::read_to_string, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::{Args, Filter, Trace, point_filters::PointFilter};

/// A RunConfig including the filters
pub(crate) struct RunConfig {
    /// The args
    pub(crate) args: Args,
    /// The filters
    pub(crate) filters: Vec<Filter>,
    /// Point filters
    pub(crate) point_filters: Vec<PointFilter>,
}

impl RunConfig {
    pub(crate) fn new(args: Args, filters: Vec<Filter>, point_filters: Vec<PointFilter>) -> Self {
        RunConfig {
            args,
            filters,
            point_filters,
        }
    }
}

/// The json type to filter
#[derive(Debug, Deserialize)]
pub(crate) struct JsonFilterDescription {
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

/// A RunConfig which we can read from a file
/// because we need JsonFilterDescription instead of filters
#[derive(Debug, Deserialize)]
pub(crate) struct RunConfigJson {
    pub(crate) args: Args,
    #[serde(default)]
    pub(crate) filters: Vec<JsonFilterDescription>,
    #[serde(default)]
    pub(crate) point_filters: Vec<PointFilter>,
}

impl From<RunConfigJson> for RunConfig {
    fn from(value: RunConfigJson) -> Self {
        RunConfig {
            args: value.args,
            filters: value.filters.into_iter().map(|f| f.into()).collect(),
            point_filters: value.point_filters,
        }
    }
}

/// read a run file into runs.
pub(crate) fn read_run_file(path: &PathBuf) -> Result<Vec<RunConfig>> {
    let file_content = read_to_string(path)?;
    let jd = &mut json5::Deserializer::from_str(&file_content)
        .context("Could not read runconfig file")?;

    let result: Result<Vec<RunConfigJson>, _> = serde_path_to_error::deserialize(jd);
    if let Err(err) = result {
        let path = err.path().to_string();
        Err(anyhow!(
            "Could not decode runfile: error {:?}, path: {:?}",
            err.inner(),
            path
        ))
    } else {
        let result = result.unwrap();
        if !(result.iter().all(|r| r.args.bencher) || result.iter().all(|r| !r.args.bencher)) {
            return Err(anyhow!(
                "We currently do not support a mixture of bencher and non-bencher runs"
            ));
        }
        result
            .into_iter()
            .map(|r| {
                if r.filters.is_empty() && r.point_filters.is_empty() {
                    Err(anyhow!(
                        "You did not specify a filter or pointfilter for at least one run."
                    ))
                } else {
                    Ok(r.into())
                }
            })
            .collect::<Result<Vec<RunConfig>>>()
    }
}
