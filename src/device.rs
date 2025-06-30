//! Functions to handle the device
use anyhow::{Context, Result, anyhow};
use regex::Regex;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    Trace,
    trace::{TimeStamp, TraceMarker},
};

/// We test if the device is reachable, i.e., the list of hdc list targets is non empty.
/// It can happen that another IDE is connected to it and then we cannot reach it (and no command fails)
pub(crate) fn is_device_reachable() -> Result<bool> {
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    let cmd = Command::new(&hdc).args(["list", "targets"]).output()?;
    Ok(!cmd.stdout.is_empty())
}

/// We sometimes want to stop the trace because we interrupted the program
pub(crate) fn stop_tracing(buffer: u64) -> Result<()> {
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    // stop trace
    Command::new(&hdc)
        .args([
            "shell",
            "hitrace",
            "-b",
            &buffer.to_string(),
            "--trace_finish",
            "-o",
            "/data/local/tmp/ohtrace.txt",
        ])
        .output()
        .map(|_| ())
        .map_err(|_| anyhow!("Could not stop trace"))
}

#[derive(Debug)]
struct DeviceFilePaths {
    /// The file path to the file on disk
    stem: String,
    /// The file path we can access in the app
    in_app: String,
    /// The file path we can put files to
    on_device: String,
}

fn device_file_paths(file_name: &str, bundle_name: &str) -> DeviceFilePaths {
    let real_file_name = file_name.trim_start_matches("file:///");

    DeviceFilePaths {
        stem: real_file_name.to_owned(),
        in_app: format!("file:///data/storage/el2/base/cache/{real_file_name}"),
        on_device: format!("/data/app/el2/100/base/{bundle_name}/cache/{real_file_name}"),
    }
}

/// Execute the hdc commands on the device.
pub(crate) fn exec_hdc_commands(args: &crate::Args) -> Result<PathBuf> {
    let be_loud = !args.bencher && !args.quiet;
    if be_loud {
        println!("Executing hdc commands");
    }
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    // stop the app before starting the test
    Command::new(&hdc)
        .args(["shell", "aa", "force-stop", &args.bundle_name])
        .output()?;

    let url = if args.url.contains("file:///") {
        let device_file_path = device_file_paths(&args.url, &args.bundle_name);
        if !args.bencher {
            println!("{device_file_path:?}");
        }
        Command::new(&hdc)
            .args([
                "file",
                "send",
                &device_file_path.stem,
                &device_file_path.on_device,
            ])
            .output()?;
        device_file_path.in_app
    } else {
        args.url.clone()
    };

    // start trace
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

    /*
        let mut logger = Command::new(&hdc)
        .args(["shell", "hilog", "-D", "0xE0C3"])
        .stdout(Stdio::piped())
        .spawn()
        .context("Could not spawn log catcher")?;
    */

    // start the ability
    let mut cmd_args = vec![
        "shell",
        "aa",
        "start",
        "-a",
        "EntryAbility",
        "-b",
        &args.bundle_name,
        "-U",
        &url,
        "--ps=--pref",
        "js_disable_jit=true",
        "--ps=--tracing-filter",
        "trace",
    ];
    if let Some(ref v) = args.commands {
        let mut v = v.iter().map(|s| s.as_str()).collect();
        cmd_args.append(&mut v);
    }
    Command::new(&hdc).args(cmd_args).output()?;

    if be_loud {
        println!("Sleeping for {}", args.sleep);
    }
    std::thread::sleep(std::time::Duration::from_secs(args.sleep));

    // Getting app pid is a simple test if the app perhaps crashed during the benchmark / test.
    let cmd = Command::new(&hdc)
        .args(["shell", "pidof", &args.bundle_name])
        .output()
        .with_context(|| format!("Is `{}` installed?", args.bundle_name))?;
    if cmd.stdout.is_empty() {
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
        return Err(anyhow!(
            "{} did not start or crashed. Please check the application logs.",
            args.bundle_name
        ));
    }
    stop_tracing(args.trace_buffer)?;

    // getting the logs
    //let mut logs = String::new();
    //logger.kill()?;
    //logger.stdout.unwrap().read_to_string(&mut logs)?;
    //println!("{}", logs);

    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("app.ftrace");
    if be_loud {
        println!("Writing ftrace to {}", tmp_path.to_str().unwrap());
    }
    // Receive trace
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

/// There is always one trace per line
/// This means that having no matched lines is ok and returns None. Having a parsing error returns Some(Err)
fn line_to_trace(regex: &Regex, line: &str) -> Option<Result<Trace>> {
    regex
        .captures_iter(line)
        .map(|c| c.extract())
        .map(match_to_trace)
        .next()
}

/// Read a regex matched line into a trace
fn match_to_trace(
    (
        _line,
        [
            name,
            pid,
            cpu,
            time1,
            time2,
            trace_marker,
            number,
            shorthand,
            msg,
        ],
    ): (&str, [&str; 9]),
) -> Result<Trace> {
    let seconds = time1.parse()?;
    let microseconds = time2.parse()?;
    let timestamp = TimeStamp {
        seconds,
        micro: microseconds,
    };
    let trace_marker = TraceMarker::from(trace_marker)?;
    Ok(Trace {
        name: name.to_owned(),
        pid: pid.parse().unwrap(),
        cpu: cpu.parse().unwrap(),
        trace_marker,
        number: number.to_string(),
        timestamp,
        shorthand: shorthand.to_owned(),
        function: msg.to_owned(),
    })
}

/// Read a file into traces
pub(crate) fn read_file(f: &Path) -> Result<Vec<Trace>> {
    // This is more specific servo tracing with the tracing_mark_write
    // Example trace: ` org.servo.servo-44962   (  44682) [010] .... 17864.716645: tracing_mark_write: B|44682|ML: do_single_part3_compilation`
    let regex = Regex::new(
        r"^\s*(.*?)\-(\d+)\s*\(\s*(\d+)\).*?(\d+)\.(\d+): tracing_mark_write: (.)\|(\d+?)\|(.*?):(.*)\s*$",
    ).expect("Could not read regex");
    let f = File::open(f)?;
    let reader = BufReader::new(f);

    let (valid_lines, invalid_lines): (Vec<_>, Vec<_>) = reader
        .lines()
        .enumerate()
        .partition(|(_index, l)| l.is_ok());

    if !invalid_lines.is_empty() {
        println!(
            "Could not read lines {:?}",
            invalid_lines
                .iter()
                .map(|(index, _l)| index)
                .collect::<Vec<_>>()
        );
    }

    valid_lines
        .into_iter()
        .filter_map(|(_index, l)| line_to_trace(&regex, &l.unwrap()))
        .collect::<Result<Vec<Trace>>>()
        .context("Could not parse one thing")
}
