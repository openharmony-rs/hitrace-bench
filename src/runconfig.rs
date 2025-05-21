use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{Args, Filter, filter::JsonFilterDescription};

/// A RunConfig including the filters
pub(crate) struct RunConfig {
    /// The args
    pub(crate) args: Args,
    /// The filters
    pub(crate) filters: Vec<Filter>,
}

impl RunConfig {
    pub(crate) fn new(args: Args, filters: Vec<Filter>) -> Self {
        RunConfig { args, filters }
    }
}

/// A RunConfig which we can read from a file
/// because we need JsonFilterDescription instead of filters
#[derive(Deserialize)]
struct RunConfigDeserialize {
    args: Args,
    filters: Vec<JsonFilterDescription>,
}

/// read a run file into runs.
pub(crate) fn read_run_file(path: &PathBuf) -> Result<Vec<RunConfig>> {
    let file = File::open(path)
        .with_context(|| format!("Could not read run file {}", path.to_string_lossy()))?;
    let reader = BufReader::new(file);
    let res: Vec<RunConfigDeserialize> = serde_hjson::from_reader(reader)
        .context("Error in decoding run file. Please look at the specification")?;

    Ok(res
        .into_iter()
        .map(|r| RunConfig {
            args: r.args,
            filters: r.filters.into_iter().map(|f| f.into()).collect(),
        })
        .collect())
}
