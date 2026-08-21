use anyhow::anyhow;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Deserialize;

/// Default ability URI / homepage used by the servo example.
pub(crate) const SERVO_DEFAULT_URL: &str = "https://servo.org";
/// Default bundle used by the servo example.
pub(crate) const SERVO_DEFAULT_BUNDLE: &str = "org.servo.servo";

#[derive(Clone, Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Completely describes runs in the a file with the `RunConfig` json format.
    #[arg(short, long)]
    pub(crate) run_file: Option<PathBuf>,

    /// Allowed to move files to a directory on the phone.
    #[arg(long, default_value_t = false)]
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

    /// Read traces from a file
    #[arg(long)]
    pub(crate) trace_file: Option<PathBuf>,

    #[clap(subcommand)]
    per_run: Option<PerRun>,
}

impl Args {
    pub(crate) fn run_args(&self) -> Option<&RunArgs> {
        match &self.per_run {
            Some(PerRun::PerRun(run_args)) => Some(run_args),
            None => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_default(path: PathBuf) -> Args {
        Args {
            run_file: None,
            is_rooted: false,
            quiet: false,
            prepend: None,
            bencher: true,
            trace_file: Some(path),
            per_run: None,
        }
    }
}

#[derive(Clone, Debug, Subcommand)]
enum PerRun {
    PerRun(RunArgs),
}

impl TryFrom<&Args> for RunArgs {
    fn try_from(value: &Args) -> Result<Self, Self::Error> {
        value
            .run_args()
            .cloned()
            .ok_or_else(|| anyhow!("Could not convert"))
    }

    type Error = anyhow::Error;
}

#[derive(Clone, Parser, Debug, Deserialize, PartialEq, Eq)]
#[command(version, about, long_about = None)]
/// Run an app on an OpenHarmony device and collect timing information.
/// Servo remains the default example (homepage, bundle, and launch flags).
pub(crate) struct RunArgs {
    #[arg(short, long)]
    #[serde(default = "default_all_traces")]
    /// Show all collected traces for the app under test
    pub(crate) all_traces: bool,

    /// The number of tries we should have to average
    #[arg(short = 'n', long, default_value_t = 1)]
    #[serde(default = "default_tries")]
    pub(crate) tries: usize,

    /// Optional ability URI passed to `aa start -U`.
    /// Servo uses this as the homepage. When omitted, servo defaults use
    /// `https://servo.org`. Pass `--no-servo-defaults` to skip `-U`.
    #[arg(short, long)]
    #[serde(default)]
    pub(crate) url: Option<String>,

    /// Do not apply servo example defaults (homepage URI and servo-specific
    /// `aa start` flags such as `--ps=--pref`).
    #[arg(long = "no-servo-defaults", default_value_t = false)]
    #[serde(default)]
    pub(crate) no_servo_defaults: bool,

    /// Trace Buffer size in KB
    #[arg(short = 't', long, default_value_t = 524288)]
    #[serde(default = "default_trace_buffer")]
    pub(crate) trace_buffer: u64,

    /// Number of sleep seconds
    #[arg(short, long, default_value_t = 10)]
    #[serde(default = "default_sleep")]
    pub(crate) sleep: u64,

    /// Name of the app bundle to start
    #[arg(short, long, default_value_t = String::from(SERVO_DEFAULT_BUNDLE))]
    #[serde(default = "default_bundle_name")]
    pub(crate) bundle_name: String,

    /// Extra arguments forwarded to `aa start` (legacy; prefer `--app-arg` or `--`).
    #[arg(long, allow_hyphen_values = true, num_args = 1, action = clap::ArgAction::Append)]
    #[serde(default = "default_commands")]
    pub(crate) commands: Option<Vec<String>>,

    /// Extra argument forwarded to the app under test (`aa start`). Repeatable.
    #[arg(long = "app-arg", allow_hyphen_values = true, action = clap::ArgAction::Append)]
    #[serde(default)]
    pub(crate) app_args: Vec<String>,

    /// Extra arguments after `--`, forwarded to the app under test.
    #[arg(last = true, allow_hyphen_values = true)]
    #[serde(default)]
    pub(crate) extra_args: Vec<String>,

    /// Use mitmproxy. Automatically start mitmdump and kill it again after the run.
    #[arg(long, default_value_t = false)]
    #[serde(default = "default_mitmproxy")]
    pub(crate) mitmproxy: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            all_traces: default_all_traces(),
            tries: default_tries(),
            url: None,
            no_servo_defaults: false,
            trace_buffer: default_trace_buffer(),
            sleep: default_sleep(),
            bundle_name: default_bundle_name(),
            commands: default_commands(),
            app_args: Vec::new(),
            extra_args: Vec::new(),
            mitmproxy: false,
        }
    }
}

impl RunArgs {
    /// Whether servo example defaults should be applied.
    pub(crate) fn servo_defaults(&self) -> bool {
        !self.no_servo_defaults
    }

    /// Ability URI / homepage after applying servo defaults.
    ///
    /// An explicit `--url` (including an empty value) wins. Otherwise servo
    /// defaults supply `https://servo.org`.
    pub(crate) fn resolved_url(&self) -> Option<&str> {
        match self.url.as_deref() {
            Some(url) if url.is_empty() => None,
            Some(url) => Some(url),
            None if self.servo_defaults() => Some(SERVO_DEFAULT_URL),
            None => None,
        }
    }

    /// Label used in printed / bencher result names.
    pub(crate) fn result_label(&self) -> &str {
        self.resolved_url().unwrap_or(self.bundle_name.as_str())
    }

    /// App-specific arguments to append to `aa start`, from `--commands`,
    /// `--app-arg`, and `--`.
    pub(crate) fn forwarded_app_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(commands) = &self.commands {
            args.extend(commands.iter().cloned());
        }
        args.extend(self.app_args.iter().cloned());
        args.extend(self.extra_args.iter().cloned());
        args
    }
}

/// Servo-specific flags previously hardcoded into every `aa start` invocation.
pub(crate) fn servo_default_launch_flags() -> Vec<String> {
    vec![
        "--ps=--pref".to_owned(),
        "js_disable_jit=true".to_owned(),
        "--ps=--tracing-filter".to_owned(),
        "trace".to_owned(),
        "--psn=--pref=largest_contentful_paint_enabled=true".to_owned(),
    ]
}

/// Servo-specific mitmproxy prefs appended when `--mitmproxy` is used with
/// servo defaults.
pub(crate) fn servo_mitmproxy_launch_flags(proxy_port: &str) -> Vec<String> {
    vec![
        format!("--psn=--pref=network_http_proxy_uri=http://127.0.0.1:{proxy_port}"),
        format!("--psn=--pref=network_https_proxy_uri=http://127.0.0.1:{proxy_port}"),
        "--psn=--ignore-certificate-errors".to_owned(),
    ]
}

// these are for serde
fn default_all_traces() -> bool {
    false
}

fn default_tries() -> usize {
    1
}

fn default_trace_buffer() -> u64 {
    524288
}

fn default_sleep() -> u64 {
    10
}

fn default_bundle_name() -> String {
    String::from(SERVO_DEFAULT_BUNDLE)
}

fn default_commands() -> Option<Vec<String>> {
    None
}

fn default_mitmproxy() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(argv: &[&str]) -> Args {
        Args::try_parse_from(argv).expect("CLI should parse")
    }

    fn parse_run(argv: &[&str]) -> RunArgs {
        parse(argv)
            .run_args()
            .cloned()
            .expect("expected per-run arguments")
    }

    #[test]
    fn per_run_is_optional_and_url_is_not_required() {
        let args = parse(&["hitrace-bench"]);
        assert!(args.run_args().is_none());

        let run = parse_run(&["hitrace-bench", "per-run"]);
        assert_eq!(run.url, None);
        assert!(!run.no_servo_defaults);
        assert_eq!(run.resolved_url(), Some(SERVO_DEFAULT_URL));
        assert_eq!(run.result_label(), SERVO_DEFAULT_URL);
        assert_eq!(run.bundle_name, SERVO_DEFAULT_BUNDLE);
        assert!(run.forwarded_app_args().is_empty());
    }

    #[test]
    fn explicit_url_still_works_for_servo() {
        let run = parse_run(&[
            "hitrace-bench",
            "per-run",
            "--url",
            "https://example.com",
            "--bundle-name",
            "org.servo.servo",
        ]);
        assert_eq!(run.url.as_deref(), Some("https://example.com"));
        assert_eq!(run.resolved_url(), Some("https://example.com"));
        assert_eq!(run.result_label(), "https://example.com");
        assert!(run.servo_defaults());
    }

    #[test]
    fn no_servo_defaults_omits_homepage() {
        let run = parse_run(&["hitrace-bench", "per-run", "--no-servo-defaults"]);
        assert!(run.no_servo_defaults);
        assert!(!run.servo_defaults());
        assert_eq!(run.resolved_url(), None);
        assert_eq!(run.result_label(), SERVO_DEFAULT_BUNDLE);
    }

    #[test]
    fn empty_url_skips_ability_uri() {
        let run = parse_run(&["hitrace-bench", "per-run", "--url", ""]);
        assert_eq!(run.resolved_url(), None);
    }

    #[test]
    fn app_arg_passthrough_is_repeatable() {
        let run = parse_run(&[
            "hitrace-bench",
            "per-run",
            "--no-servo-defaults",
            "--bundle-name",
            "com.example.app",
            "--app-arg",
            "--ps=--exit",
            "--app-arg",
            "--foo=bar",
        ]);
        assert_eq!(
            run.forwarded_app_args(),
            vec!["--ps=--exit".to_owned(), "--foo=bar".to_owned()]
        );
        assert_eq!(run.bundle_name, "com.example.app");
        assert_eq!(run.resolved_url(), None);
    }

    #[test]
    fn trailing_double_dash_args_are_forwarded() {
        let run = parse_run(&[
            "hitrace-bench",
            "per-run",
            "--url",
            "https://servo.org",
            "--",
            "--ps=--exit",
            "--psn=--pref=foo=true",
        ]);
        assert_eq!(
            run.extra_args,
            vec!["--ps=--exit".to_owned(), "--psn=--pref=foo=true".to_owned()]
        );
        assert_eq!(
            run.forwarded_app_args(),
            vec!["--ps=--exit".to_owned(), "--psn=--pref=foo=true".to_owned()]
        );
    }

    #[test]
    fn commands_app_args_and_trailing_args_are_merged() {
        let run = parse_run(&[
            "hitrace-bench",
            "per-run",
            "--commands",
            "--legacy",
            "--app-arg",
            "--from-app-arg",
            "--",
            "--from-trailing",
        ]);
        assert_eq!(
            run.forwarded_app_args(),
            vec![
                "--legacy".to_owned(),
                "--from-app-arg".to_owned(),
                "--from-trailing".to_owned()
            ]
        );
    }

    #[test]
    fn json_commands_and_app_args_deserialize() {
        let json = r#"{
            "url": "https://www.google.com",
            "commands": ["--ps=--tracing-filter", "info"],
            "app_args": ["--ps=--exit"],
            "no_servo_defaults": true
        }"#;
        let run: RunArgs = serde_json::from_str(json).expect("json should deserialize");
        assert_eq!(run.url.as_deref(), Some("https://www.google.com"));
        assert!(run.no_servo_defaults);
        assert_eq!(run.resolved_url(), Some("https://www.google.com"));
        assert_eq!(
            run.forwarded_app_args(),
            vec![
                "--ps=--tracing-filter".to_owned(),
                "info".to_owned(),
                "--ps=--exit".to_owned()
            ]
        );
    }

    #[test]
    fn json_without_url_keeps_servo_default_homepage() {
        let run: RunArgs = serde_json::from_str("{}").expect("empty run_args should deserialize");
        assert_eq!(run.url, None);
        assert_eq!(run.resolved_url(), Some(SERVO_DEFAULT_URL));
        assert!(run.servo_defaults());
    }
}
