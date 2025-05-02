use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Run servo on an open harmony device and collect timing information
pub(crate) struct Args {
    #[arg(short, long)]
    /// Show all traces for servo
    pub(crate) all_traces: bool,

    /// The number of tries we should have to average
    #[arg(short = 'n', long, default_value_t = 1)]
    pub(crate) tries: usize,

    /// The homepage we try to load
    #[arg(short = 'p', long, default_value_t = String::from("https://servo.org"))]
    pub(crate) homepage: String,

    /// Trace Buffer size in KB
    #[arg(short = 't', long, default_value_t = 524288)]
    pub(crate) trace_buffer: u64,

    /// Number of sleep seconds
    #[arg(short, long, default_value_t = 10)]
    pub(crate) sleep: u64,

    /// Stay silent and only return the miliseconds in a list
    #[arg(short, long, default_value_t = false)]
    pub(crate) computer_output: bool,

    /// Name of the app bundle to start
    #[arg(short, long, default_value_t = String::from("org.servo.servo"))]
    pub(crate) bundle_name: String,

    /// Use Bencher output format
    #[arg(long, default_value_t = false)]
    pub(crate) bencher: bool,
}
