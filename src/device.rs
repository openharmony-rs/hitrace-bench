//! Functions to handle the device
use anyhow::{Context, Result, anyhow};
use log::info;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    str::FromStr,
    thread,
    time::Duration,
};

use crate::args::{RunArgs, servo_default_launch_flags, servo_mitmproxy_launch_flags};

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

/// Take a screenshot and return the Path on the host, not the phone. Currently the path is fixed.
fn take_screenshot() -> Result<PathBuf> {
    let hdc = which::which("hdc").context("Is hdc in the path?")?;
    const DEVICE_PATH: &str = "/data/local/tmp/servo.jpeg";
    // if the delete does not work we do not really care
    let _ = Command::new(&hdc)
        .args(["rm", "-f", DEVICE_PATH])
        .output()
        .map(|_| ());
    Command::new(&hdc)
        .args(["shell", "snapshot_display", "-f", DEVICE_PATH])
        .output()
        .map(|_| ())
        .map_err(|_| anyhow!("Could not take screenshot"))?;
    Command::new(&hdc)
        .args(["file", "recv", DEVICE_PATH, "/tmp/servo.jpeg"])
        .output()
        .map(|_| ())
        .map_err(|_| anyhow!("Could not transfer screenshot"))?;

    PathBuf::from_str("/tmp/servo.jpg").map_err(|_| anyhow!("Could not convert file"))
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

/// Build `hdc aa start` arguments for the app under test.
///
/// `ability_uri` is the optional `-U` value (already rewritten for `file://` uploads).
/// Servo-specific launch flags are included only when servo defaults are enabled.
pub(crate) fn ability_start_args(run_args: &RunArgs, ability_uri: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "shell".to_owned(),
        "aa".to_owned(),
        "start".to_owned(),
        "-a".to_owned(),
        "EntryAbility".to_owned(),
        "-b".to_owned(),
        run_args.bundle_name.clone(),
    ];
    if let Some(uri) = ability_uri {
        args.push("-U".to_owned());
        args.push(uri.to_owned());
    }
    if run_args.servo_defaults() {
        args.extend(servo_default_launch_flags());
    }
    args.extend(run_args.forwarded_app_args());
    if run_args.mitmproxy && run_args.servo_defaults() {
        args.extend(servo_mitmproxy_launch_flags(PROXY_PORT));
    }
    args
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

    let ability_uri = if let Some(url) = run_args.resolved_url() {
        if url.contains("file:///") {
            let device_file_path = device_file_paths(url, &run_args.bundle_name, is_rooted);

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
            Some(device_file_path.in_app)
        } else {
            Some(url.to_owned())
        }
    } else {
        None
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
    Command::new(&hdc)
        .args(ability_start_args(run_args, ability_uri.as_deref()))
        .output()?;
    // Getting app pid is a simple test if the app perhaps crashed during the benchmark / test.
    // Because teh app might finish rendering really fast, we need to be fast to check for the pid.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let cmd = Command::new(&hdc)
        .args(["shell", "pidof", &run_args.bundle_name])
        .output()
        .with_context(|| format!("Is `{}` installed?", run_args.bundle_name))?;
    info!("Sleeping for {}", run_args.sleep);
    std::thread::sleep(std::time::Duration::from_secs(run_args.sleep));

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
        let path = take_screenshot()?;
        println!("Took screenshot {path:?}");
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

        let proxy = MitmProxy(mitmdump_cmd.stdout(Stdio::piped()).spawn()?);

        // Mitmproxy needs a bit to spawn and returning immedieately might be missing it.
        thread::sleep(Duration::from_secs(1));

        Ok(proxy)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{SERVO_DEFAULT_BUNDLE, SERVO_DEFAULT_URL, servo_default_launch_flags};

    #[test]
    fn servo_defaults_keep_homepage_and_servo_flags() {
        let run_args = RunArgs::default();
        let args = ability_start_args(&run_args, run_args.resolved_url());

        assert_eq!(
            &args[..8],
            [
                "shell",
                "aa",
                "start",
                "-a",
                "EntryAbility",
                "-b",
                SERVO_DEFAULT_BUNDLE,
                "-U",
            ]
        );
        assert_eq!(args[8], SERVO_DEFAULT_URL);
        for flag in servo_default_launch_flags() {
            assert!(
                args.contains(&flag),
                "servo default launch should include {flag}"
            );
        }
    }

    #[test]
    fn generic_app_does_not_require_homepage_or_servo_flags() {
        let run_args = RunArgs {
            no_servo_defaults: true,
            bundle_name: "com.example.app".to_owned(),
            app_args: vec!["--foo".to_owned(), "bar".to_owned()],
            extra_args: vec!["--baz".to_owned()],
            ..RunArgs::default()
        };
        let args = ability_start_args(&run_args, run_args.resolved_url());

        assert_eq!(
            args,
            vec![
                "shell".to_owned(),
                "aa".to_owned(),
                "start".to_owned(),
                "-a".to_owned(),
                "EntryAbility".to_owned(),
                "-b".to_owned(),
                "com.example.app".to_owned(),
                "--foo".to_owned(),
                "bar".to_owned(),
                "--baz".to_owned(),
            ]
        );
        assert!(!args.contains(&"-U".to_owned()));
        assert!(!args.iter().any(|arg| arg.contains("js_disable_jit")));
        assert!(!args.iter().any(|arg| arg.contains("tracing-filter")));
    }

    #[test]
    fn mitmproxy_flags_are_servo_specific() {
        let servo = RunArgs {
            mitmproxy: true,
            ..RunArgs::default()
        };
        let servo_args = ability_start_args(&servo, servo.resolved_url());
        assert!(
            servo_args
                .iter()
                .any(|arg| arg.contains("network_http_proxy_uri"))
        );

        let generic = RunArgs {
            no_servo_defaults: true,
            mitmproxy: true,
            app_args: vec!["--psn=--custom-proxy".to_owned()],
            ..RunArgs::default()
        };
        let generic_args = ability_start_args(&generic, generic.resolved_url());
        assert!(generic_args.contains(&"--psn=--custom-proxy".to_owned()));
        assert!(
            !generic_args
                .iter()
                .any(|arg| arg.contains("network_http_proxy_uri"))
        );
    }
}
