use ::time::{Duration, PrimitiveDateTime, format_description};
use anyhow::{Context, Result, anyhow};
use clap::{Parser, command};
use regex::Regex;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};
use time::{Time, format_description::BorrowedFormatItem};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Run servo on an open harmony device and collect timing information
struct Args {
    #[arg(short, long)]
    /// Show all traces for servo
    all_traces: bool,

    /// The number of tries we should have to average
    #[arg(short, long, default_value_t = 1)]
    tries: usize,

    /// The homepage we try to load
    #[arg(short = 'p', long, default_value_t = String::from("https://servo.org"))]
    homepage: String,

    /// Trace Buffer size in KB
    #[arg(short = 'b', long, default_value_t = 524288)]
    trace_buffer: u64,

    /// Number of sleep seconds
    #[arg(short, long, default_value_t = 5)]
    sleep: u64,
}

#[derive(Debug)]
/// A parsed trace
struct Trace {
    /// Name of the program, i.e., org.servo.servo
    name: String,
    /// pid
    pid: u64,
    /// the cpu it ran on
    cpu: u64,
    /// timestamp of the trace
    timestamp: Time,
    /// Some shorthand code
    shorthand: String,
    /// Full function name
    function: String,
}

/// Execute the hdc commands on the device.
fn exec_hdc_commands(args: &Args) -> Result<PathBuf> {
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
            &args.trace_buffer.to_string(),
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

    println!("Sleeping for {}", args.sleep);
    std::thread::sleep(std::time::Duration::from_secs(args.sleep));

    Command::new(&hdc)
        .args([
            "shell",
            "hitrace",
            "-b",
            &args.trace_buffer.to_string(),
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

/// Read a file into traces
fn read_file(f: &Path) -> Result<Vec<Trace>> {
    let f = File::open(f)?;
    let reader = BufReader::new(f);
    reader
        .lines()
        .map(|l| l.unwrap())
        .filter_map(|l| line_to_trace(&l))
        .collect::<Result<Vec<Trace>>>()
        .context("Could not parse one thing")
}

/// There is always one trace per line
/// This means that having no matched lines is ok and returns None. Having a parsing error returns Some(Err)
fn line_to_trace(line: &str) -> Option<Result<Trace>> {
    SERVO_TRACE_POINT_REGEX
        .captures_iter(line)
        .map(|c| c.extract())
        .map(match_to_trace)
        .next()
}

/// Read a regex matched line into a trace
fn match_to_trace(
    (l, [name, pid, cpu, time1, time2, shorthand, line]): (&str, [&str; 7]),
) -> Result<Trace> {
    let seconds = time1.parse()?;
    let microseconds = time2.parse()?;
    let timestamp = Time::from_hms(0, 0, 0)?
        + Duration::microseconds(seconds)
        + Duration::microseconds(microseconds);
    Ok(Trace {
        name: name.to_owned(),
        pid: pid.parse().unwrap(),
        cpu: cpu.parse().unwrap(),
        timestamp,
        shorthand: shorthand.to_owned(),
        function: line.to_owned(),
    })
}

#[derive(Debug)]
/// the difference in timing, represented by two integers, representing major and minor difference
struct Difference<'a> {
    /// Major and minor differences
    difference: Duration,
    /// The name of the difference
    name: &'a str,
}

/// Way to construct filters
struct Filters<'a> {
    /// A name for the filter that will be output
    name: &'a str,
    /// A function taking a trace and deciding if it should be the start of the timing
    first: fn(&Trace) -> bool,
    /// A function taking a trace and deciding if it should be the end of the timing
    last: fn(&Trace) -> bool,
}

/// Look through the traces and find all timing differences coming from the filters
fn find_notable_differences<'a>(
    v: Vec<Trace>,
    filters: &'a Vec<Filters>,
) -> Result<Vec<Difference<'a>>> {
    let mut differences = Vec::new();
    for f in filters {
        let first = v.iter().filter(|t| (f.first)(t)).collect::<Vec<&Trace>>();
        let last = v.iter().filter(|t| (f.last)(t)).collect::<Vec<&Trace>>();
        if first.len() != 1 || last.len() != 1 {
            return Err(anyhow!(
                "Your filter functions are not specific or over specific, we got the following number of results: name: {}, first: {}, last: {}",
                f.name,
                first.len(),
                last.len()
            ));
        } else {
            differences.push(Difference {
                name: f.name,
                difference: last[0].timestamp - first[0].timestamp,
            })
        }
    }
    Ok(differences)
}

/// Print the differences
fn print_differences(args: &Args, diff: Vec<Difference>) {
    let mut hash: HashMap<&str, Vec<Duration>> = HashMap::new();
    for i in &diff {
        hash.entry(i.name)
            .and_modify(|v| v.push(i.difference))
            .or_insert(vec![(i.difference)]);
    }

    for (key, val) in hash.iter() {
        let avg = val
            .iter()
            .sum::<Duration>()
            .checked_div(args.tries as i32)
            .unwrap();
        let min = val.iter().min().unwrap();
        let max = val.iter().max().unwrap();
        println!("----name avg min max------------------------------");
        println!("{}: {:?} {:?} {:?}", key, avg, min, max);
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut traces = Vec::new();
    for i in 1..args.tries + 1 {
        println!("Running test {}", i);
        let log_path = exec_hdc_commands(&args)?;
        let mut new_traces = read_file(&log_path)?;
        traces.append(&mut new_traces);
    }

    let filters = vec![Filters {
        name: "Startup",
        first: |t| t.shorthand == "H" && t.function.contains("panda::JSNApi::PostFork"),
        last: |t| t.shorthand == "H" && t.function == "PageLoadEndedPrompt",
    }];

    if args.all_traces {
        for i in &traces {
            println!("{:?}", i);
        }
        println!("----------------------------------------------------------\n\n");
    }
    let differences = find_notable_differences(traces, &filters).context(
        "Something went wrong with finding the traces, look up to see what probably went wrong",
    )?;

    print_differences(&args, differences);

    Ok(())
}
