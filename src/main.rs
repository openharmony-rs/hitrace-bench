use anyhow::{Context, Result, anyhow};
use clap::{Parser, command};
use regex::Regex;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
    time::Duration,
};

#[derive(Debug)]
struct TimeStamp {
    major: u64,
    minor: u64,
}

impl TimeStamp {
    fn difference(&self, other: &TimeStamp) -> (i64, i64) {
        (
            other.major as i64 - self.major as i64,
            other.minor as i64 - self.minor as i64,
        )
    }
}

#[derive(Debug)]
struct Trace {
    name: String,
    pid: u64,
    cpu: u64,
    timestamp: TimeStamp,
    shorthand: String,
    function: String,
}

static TRACE_BUFFER_IN_KB: &str = "524288";
static HOMEPAGE: &str = "https://servo.org";
static SLEEP_SECONDS: u64 = 5;

fn exec_hdc_commands() -> Result<PathBuf> {
    println!("Executing hdc commands");
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    Command::new(&hdc)
        .args(["shell", "aa", "force-stop", "org.servo.servo"])
        .output()?;

    Command::new(&hdc)
        .args([
            "shell",
            "hitrace",
            "-b",
            TRACE_BUFFER_IN_KB,
            "app",
            "graphic",
            "ohos",
            "freq",
            "idle",
            "memory",
            "--trace_begin",
        ])
        .output()?;
    Command::new(&hdc)
        .args([
            "shell",
            "aa",
            "start",
            "-a",
            "EntryAbility",
            "-b",
            "org.servo.servo",
            "-U",
            "HOMEPAGE",
            "--ps=--pref",
            "js_disable_jit=true",
        ])
        .output()?;

    println!("Sleeping for {}", SLEEP_SECONDS);
    std::thread::sleep(Duration::from_secs(SLEEP_SECONDS));

    Command::new(&hdc)
        .args([
            "shell",
            "hitrace",
            "-b",
            TRACE_BUFFER_IN_KB,
            "--trace_finish",
            "-o",
            "/data/local/tmp/ohtrace.txt",
        ])
        .output()?;
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("servo.ftrace");
    println!("Writing ftrace to {}", tmp_path.to_str().unwrap());
    Command::new(&hdc)
        .args([
            "file",
            "recv",
            "/data/local/tmp/ohtrace.txt",
            tmp_path.to_str().unwrap(),
        ])
        .output()?;
    Ok(tmp_path)
}

/// This regex matches the general tracings with rss_stat.
/// Example trace: `org.servo.servo-44682   (  44682) [006] .... 17863.362316: rss_stat: mm_id=537018 curr=1 member=2 size=68227072B``
//static SERVO_TRACE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
//    Regex::new(r"^(.*?servo)\-(\d+)\s*\(\s*(\d+)\).*?(\d+)\.(\d+):(.*)$").unwrap()
//});

/// This is more specific servo tracing with the tracing_mark_write
/// Example trace: `org.servo.servo-44962   (  44682) [010] .... 17864.716645: tracing_mark_write: B|44682|ML: do_single_part3_compilation`
static SERVO_TRACE_POINT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.*?servo)\-(\d+)\s*\(\s*(\d+)\).*?(\d+)\.(\d+): tracing_mark_write: ........(.*?):(.*?)\s*$").unwrap()
});

fn read_file(f: &Path) -> Result<Vec<Trace>> {
    let f = File::open(f)?;
    let reader = BufReader::new(f);
    let traces = reader
        .lines()
        .map(|l| l.unwrap())
        .map(|l| line_to_trace(&l))
        .flatten()
        .collect::<Vec<Trace>>();

    Ok(traces)
}

/// There is always one trace per line
fn line_to_trace(line: &str) -> Vec<Trace> {
    SERVO_TRACE_POINT_REGEX
        .captures_iter(&line)
        .map(|c| c.extract())
        .map(match_to_trace)
        .collect()
}

fn match_to_trace(
    (_, [name, pid, cpu, time1, time2, shorthand, line]): (&str, [&str; 7]),
) -> Trace {
    Trace {
        name: name.to_owned(),
        pid: pid.parse().unwrap(),
        cpu: cpu.parse().unwrap(),
        timestamp: TimeStamp {
            major: time1.parse().unwrap(),
            minor: time2.parse().unwrap(),
        },
        shorthand: shorthand.to_owned(),
        function: line.to_owned(),
    }
}

#[derive(Debug)]
struct Difference<'a> {
    difference: (i64, i64),
    name: &'a str,
}

struct Filters<'a> {
    name: &'a str,
    first: fn(&Trace) -> bool,
    last: fn(&Trace) -> bool,
}

fn find_notable_differences<'a>(
    v: Vec<Trace>,
    filters: &'a Vec<Filters>,
) -> Vec<Result<Difference<'a>>> {
    filters.iter().map(|f| {
        let first = v.iter().filter(|t| (f.first)(t)).collect::<Vec<&Trace>>();
        let last = v.iter().filter(|t| (f.last)(t)).collect::<Vec<&Trace>>();
        if first.len()!=1 || last.len()!=1 {
            Err(anyhow!("Your filter functions are not specific or over specific, we got the following number of results: first: {}, last: {}", first.len(), last.len()))
        } else {
            Ok(Difference {
                name: f.name,
                difference: last[0].timestamp.difference(&first[0].timestamp)
            })
        }
    }).collect()
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    all_traces: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_path = exec_hdc_commands()?;
    let traces = read_file(&log_path)?;

    let filters = vec![Filters {
        name: "General",
        first: |t| t.shorthand == "H" && t.function.contains("SetApplicationStatus"),
        last: |t| t.shorthand == "H" && t.function == "PageLoadEndedPrompt",
    }];

    if args.all_traces {
        for i in &traces {
            println!("{:?}", i);
        }
    }
    let filtered_results = find_notable_differences(traces, &filters);

    println!("{:?}", filtered_results);
    Ok(())
}
