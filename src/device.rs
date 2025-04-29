//! Functions to handle the device
use anyhow::{Context, Result, anyhow};
use std::{path::PathBuf, process::Command};

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

/// Execute the hdc commands on the device.
pub(crate) fn exec_hdc_commands(args: &crate::Args) -> Result<PathBuf> {
    if !args.computer_output && !args.bencher {
        println!("Executing hdc commands");
    }
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    // stop the app before starting the test
    Command::new(&hdc)
        .args(["shell", "aa", "force-stop", &args.bundle_name])
        .output()?;
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
    // start the ability
    Command::new(&hdc)
        .args([
            "shell",
            "aa",
            "start",
            "-a",
            "EntryAbility",
            "-b",
            &args.bundle_name,
            "-U",
            &args.homepage,
            "--ps=--pref",
            "js_disable_jit=true",
        ])
        .output()?;

    if !args.computer_output && !args.bencher {
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
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("app.ftrace");
    if !args.computer_output && !args.bencher {
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
