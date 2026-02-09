//! Functions to handle the device
use anyhow::{Context, Result, anyhow};
use log::info;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use crate::args::RunArgs;

const PROXY_PORT: &str = "8080";

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

    let _mitmproxy = if run_args.mitmproxy {
        MitmProxy::new().ok()
    } else {
        None
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
    let mut ability_start_arg = Command::new(&hdc);
    ability_start_arg.args([
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
    ]);
    if let Some(ref v) = run_args.commands {
        for i in v {
            ability_start_arg.arg(i);
        }
    }
    if run_args.mitmproxy {
        ability_start_arg.args([
            format!(
                "--psn=--pref=network_http_proxy_uri=http://127.0.0.1:{}",
                PROXY_PORT
            ),
            format!(
                "--psn=--pref=network_https_proxy_uri=http://127.0.0.1:{}",
                PROXY_PORT
            ),
        ]);
        ability_start_arg.arg("--psn=--ignore-certificate-errors");
    }

    ability_start_arg.output()?;
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

struct MitmProxy(Child);

impl MitmProxy {
    fn new() -> Result<Self> {
        let hdc = which::which("hdc").context("Is hdc in the path?")?;
        let ports_forwarded = Command::new(&hdc).args(["fport", "ls"]).output()?;
        let output =
            String::from_utf8(ports_forwarded.stdout).context("Hdc reported weird characters")?;
        if !output.contains(PROXY_PORT) {
            Command::new(&hdc)
                .args([
                    "rport".into(),
                    format!("tcp:{}", PROXY_PORT),
                    format!("tcp:{}", PROXY_PORT),
                ])
                .output()
                .context("Could not forward port")?;
        }

        let mitmdump = which::which("mitmdump").context("Is mitmdump in path?")?;
        let mut mitmdump_cmd = Command::new(mitmdump);
        mitmdump_cmd.args(["--set", "ssl_insecure=true", "-p", PROXY_PORT]);

        if let Ok(proxy) = std::env::var("http_proxy") {
            mitmdump_cmd.arg("--mode");
            mitmdump_cmd.arg(format!("upstream:{}", proxy));
            info!("Starting mitmdump with proxy {:?}", proxy);
        }
        mitmdump_cmd.stdout(Stdio::piped());
        mitmdump_cmd.env_clear(); // Does not hurt and prevents secret leaks, I hope.

        Ok(MitmProxy(mitmdump_cmd.stdout(Stdio::piped()).spawn()?))
    }
}

impl Drop for MitmProxy {
    fn drop(&mut self) {
        if self.0.kill().is_err() {
            log::error!("Problem killing mitmproxy");
        }
        if self.0.wait().is_err() {
            log::error!("Could not wait on killed process");
        }
    }
}
