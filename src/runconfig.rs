use std::{fmt::Display, fs::read_to_string, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::{
    Filter, Trace,
    args::{Args, RunArgs},
    point_filters::PointFilter,
};

/// A RunConfig including the filters
pub(crate) struct RunConfig {
    /// The program args
    pub(crate) args: Args,
    /// The args
    pub(crate) run_args: RunArgs,
    /// The filters
    pub(crate) filters: Vec<Filter>,
    /// Point filters
    pub(crate) point_filters: Vec<PointFilter>,
}

impl Display for RunConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "args {:?}, filters {:?}, point_filters {:?}",
            self.args,
            self.filters
                .iter()
                .map(|f| f.name.to_owned())
                .collect::<Vec<_>>(),
            self.point_filters
        )
    }
}

impl RunConfig {
    pub(crate) fn new(
        args: Args,
        run_args: RunArgs,
        filters: Vec<Filter>,
        point_filters: Vec<PointFilter>,
    ) -> Self {
        RunConfig {
            args,
            run_args,
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
    pub(crate) run_args: RunArgs,
    #[serde(default)]
    pub(crate) filters: Vec<JsonFilterDescription>,
    #[serde(default)]
    pub(crate) point_filters: Vec<PointFilter>,
}

/// Uses `Args` and `RunConfigJson` to create a `RunConfig`
pub(crate) fn into_run_config(args: Args, run_config_json: RunConfigJson) -> RunConfig {
    RunConfig {
        args,
        run_args: run_config_json.run_args,
        filters: run_config_json
            .filters
            .into_iter()
            .map(|f| f.into())
            .collect(),
        point_filters: run_config_json
            .point_filters
            .into_iter()
            .map(PointFilter::finalize)
            .collect(),
    }
}

/// read a run file into runs.
pub(crate) fn read_run_file(path: &PathBuf, args: &Args) -> Result<Vec<RunConfig>> {
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
        result
            .into_iter()
            .map(|r| {
                if r.filters.is_empty() && r.point_filters.is_empty() {
                    Err(anyhow!(
                        "You did not specify a filter or pointfilter for at least one run."
                    ))
                } else {
                    Ok(into_run_config(args.clone(), r))
                }
            })
            .collect::<Result<Vec<RunConfig>>>()
    }
}
