use anyhow::anyhow;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Deserialize;

#[derive(Clone, Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Completely describes runs in the a file with the `RunConfig` json format.
    #[arg(short, long)]
    pub(crate) run_file: Option<PathBuf>,

    /// Allowed to move files to a directory on the phone.
    #[arg(short, long, default_value_t = false)]
    pub(crate) is_rooted: bool,

    /// Keep quiet and only print the output
    #[arg(short, long, default_value_t = false)]
    pub(crate) quiet: bool,

    /// This is a string we prepend to every target
    #[arg(short, long)]
    pub(crate) prepend: Option<String>,

    /// Use Bencher output format. This also does a couple of other things.
    /// See the description in `bencher.rs`
    #[arg(long, default_value_t = false)]
    pub(crate) bencher: bool,

    #[clap(subcommand)]
    per_run: Option<PerRun>,
}

#[derive(Clone, Debug, Subcommand)]
enum PerRun {
    PerRun(RunArgs),
}

impl TryFrom<&Args> for RunArgs {
    fn try_from(value: &Args) -> Result<Self, Self::Error> {
        match &value.per_run {
            Some(PerRun::PerRun(run_args)) => Ok(run_args.to_owned()),
            None => Err(anyhow!("Could not convert")),
        }
    }

    type Error = anyhow::Error;
}

#[derive(Clone, Parser, Debug, Deserialize)]
#[command(version, about, long_about = None)]
/// Run servo on an open harmony device and collect timing information
pub(crate) struct RunArgs {
    #[arg(short, long)]
    #[serde(default = "default_all_traces")]
    /// Show all traces for servo
    pub(crate) all_traces: bool,

    /// The number of tries we should have to average
    #[arg(short = 'n', long, default_value_t = 1)]
    #[serde(default = "default_tries")]
    pub(crate) tries: usize,

    /// The homepage we try to load
    #[arg(short, long, default_value_t = String::from("https://servo.org"))]
    #[serde(default = "default_url")]
    pub(crate) url: String,

    /// Trace Buffer size in KB
    #[arg(short = 't', long, default_value_t = 524288)]
    #[serde(default = "default_trace_buffer")]
    pub(crate) trace_buffer: u64,

    /// Number of sleep seconds
    #[arg(short, long, default_value_t = 10)]
    #[serde(default = "default_sleep")]
    pub(crate) sleep: u64,

    /// Name of the app bundle to start
    #[arg(short, long, default_value_t = String::from("org.servo.servo"))]
    #[serde(default = "default_bundle_name")]
    pub(crate) bundle_name: String,

    /// Read traces from a file
    #[arg(long)]
    #[serde(skip)]
    pub(crate) trace_file: Option<PathBuf>,

    /// These will be directly given to the hdc shell start command at the end.
    #[arg(long, trailing_var_arg(true), allow_hyphen_values(true), num_args=0..)]
    #[serde(default = "default_commands")]
    pub(crate) commands: Option<Vec<String>>,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            all_traces: default_all_traces(),
            tries: default_tries(),
            url: default_url(),
            trace_buffer: default_trace_buffer(),
            sleep: default_sleep(),
            bundle_name: default_bundle_name(),
            trace_file: None,
            commands: default_commands(),
        }
    }
}

// these are for serde
fn default_all_traces() -> bool {
    false
}

fn default_tries() -> usize {
    1
}

fn default_url() -> String {
    String::from("https://servo.org")
}

fn default_trace_buffer() -> u64 {
    524288
}

fn default_sleep() -> u64 {
    10
}

fn default_bundle_name() -> String {
    String::from("org.servo.servo")
}

fn default_commands() -> Option<Vec<String>> {
    None
}
