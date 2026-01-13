//! Functions to handle the device
use anyhow::{Context, Result, anyhow};
use log::info;
use std::{path::PathBuf, process::Command};

use crate::args::RunArgs;

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

/// Depending on root or non-rooted we will have different file paths. This gives us these paths.
fn device_file_paths(file_name: &str, bundle_name: &str, is_rooted: bool) -> DeviceFilePaths {
    let real_file_name = file_name.trim_start_matches("file:///");

    if is_rooted {
        DeviceFilePaths {
            stem: real_file_name.to_owned(),
            in_app: format!("file:///data/storage/el2/base/cache/{real_file_name}"),
            on_device: format!("/data/app/el2/100/base/{bundle_name}/cache/{real_file_name}"),
        }
    } else {
        DeviceFilePaths {
            stem: real_file_name.to_owned(),
            in_app: format!(
                "file:///data/storage/el1/bundle/servoshell/resources/resfile/{real_file_name}"
            ),
            on_device: String::new(),
        }
    }
}

/// Execute the hdc commands on the device.
pub(crate) fn exec_hdc_commands(run_args: &RunArgs, is_rooted: bool) -> Result<PathBuf> {
    info!("Executing hdc commands");
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    // stop the app before starting the test
    Command::new(&hdc)
        .args(["shell", "aa", "force-stop", &run_args.bundle_name])
        .output()
        .context("Could not execute hdc")?;

    let url = if run_args.url.contains("file:///") {
        let device_file_path = device_file_paths(&run_args.url, &run_args.bundle_name, is_rooted);

        if is_rooted {
            info!(
                "Uploading to {} visible as {}",
                device_file_path.on_device, device_file_path.in_app
            );
            Command::new(&hdc)
                .args([
                    "file",
                    "send",
                    &device_file_path.stem,
                    &device_file_path.on_device,
                ])
                .output()?;
        }
        device_file_path.in_app
    } else {
        run_args.url.clone()
    };

    // start trace
    Command::new(&hdc)
        .args([
            "shell",
            "hitrace",
            "-b",
            &run_args.trace_buffer.to_string(),
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
    let mut cmd_args = vec![
        "shell",
        "aa",
        "start",
        "-a",
        "EntryAbility",
        "-b",
        &run_args.bundle_name,
        "-U",
        &url,
        "--ps=--pref",
        "js_disable_jit=true",
        "--ps=--tracing-filter",
        "trace",
        "--psn=--pref=largest_contentful_paint_enabled=true",
    ];
    if let Some(ref v) = run_args.commands {
        let mut v = v.iter().map(|s| s.as_str()).collect();
        cmd_args.append(&mut v);
    }
    Command::new(&hdc).args(cmd_args).output()?;
    info!("Sleeping for {}", run_args.sleep);
    std::thread::sleep(std::time::Duration::from_secs(run_args.sleep));

    // Getting app pid is a simple test if the app perhaps crashed during the benchmark / test.
    let cmd = Command::new(&hdc)
        .args(["shell", "pidof", &run_args.bundle_name])
        .output()
        .with_context(|| format!("Is `{}` installed?", run_args.bundle_name))?;
    if cmd.stdout.is_empty() {
        Command::new(&hdc)
            .args([
                "shell",
                "hitrace",
                "-b",
                &run_args.trace_buffer.to_string(),
                "--trace_finish",
                "-o",
                "/data/local/tmp/ohtrace.txt",
            ])
            .output()?;
        return Err(anyhow!(
            "{} did not start or crashed. Please check the application logs.",
            run_args.bundle_name
        ));
    }
    stop_tracing(run_args.trace_buffer)?;

    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("app.ftrace");
    info!("Writing ftrace to {}", tmp_path.to_str().unwrap());
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
