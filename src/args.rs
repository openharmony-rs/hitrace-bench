use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

#[derive(Parser, Debug, Deserialize)]
#[command(version, about, long_about = None)]
/// Run servo on an open harmony device and collect timing information
pub(crate) struct Args {
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

    /// Use Bencher output format. This also does a couple of other things.
    /// See the description in `bencher.rs`
    #[arg(long, default_value_t = false)]
    #[serde(default = "default_bencher")]
    pub(crate) bencher: bool,

    /// A json file describing the filters
    #[arg(short, long)]
    #[serde(skip)]
    pub(crate) filter_file: Option<PathBuf>,

    /// Completely describes runs in the a file with the `RunConfig` json format.
    #[arg(short, long)]
    #[serde(skip)]
    pub(crate) run_file: Option<PathBuf>,

    /// Read traces from a file
    #[arg(long)]
    #[serde(skip)]
    pub(crate) trace_file: Option<PathBuf>,

    /// Keep quiet and only print the output
    #[arg(short, long)]
    #[serde(skip)]
    pub(crate) quiet: bool,

    /// Allowed to move files to a directory on the phone.
    #[arg(short, long)]
    #[serde(default = "default_is_rooted")]
    pub(crate) is_rooted: bool,

    /// These will be directly given to the hdc shell start command at the end.
    #[arg(long, trailing_var_arg(true), allow_hyphen_values(true), num_args=0..)]
    #[serde(default = "default_commands")]
    pub(crate) commands: Option<Vec<String>>,
}

#[cfg(test)]
impl Args {
    pub(crate) fn new(bencher: bool, quiet: bool) -> Self {
        Args {
            all_traces: false,
            tries: 1,
            url: String::from(""),
            trace_buffer: 1,
            sleep: 1,
            bundle_name: String::from(""),
            bencher,
            filter_file: None,
            trace_file: None,
            is_rooted: false,
            quiet,
            run_file: None,
            commands: None,
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

fn default_bencher() -> bool {
    false
}

fn default_commands() -> Option<Vec<String>> {
    None
}

fn default_is_rooted() -> bool {
    false
}
