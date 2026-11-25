//! The scripted fake `openhuman-core`: `run --headless-api --host 127.0.0.1
//! --port <n>` serves the fake surface from `surface.rs` on that port, with
//! the token and workspace from the environment the runner gives the real
//! one. `CONTROL` names the control directory; a test's wrapper script sets
//! it. Asked through `<control>/escape`, it leaves a grandchild in a session
//! of its own, which reports its pid to `<control>/escapee`.
#[path = "../support/http.rs"]
mod http;
#[path = "surface.rs"]
mod surface;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let [mode, report] = arguments.as_slice()
        && mode == "escapee"
    {
        return escapee(Path::new(report));
    }
    // The real binary is never run with its sandbox off, so neither is this.
    if std::env::var_os("OPENHUMAN_SANDBOX").is_some() {
        return ExitCode::from(3);
    }
    let port = arguments
        .windows(2)
        .find(|pair| pair[0] == "--port")
        .and_then(|pair| pair[1].parse::<u16>().ok());
    let variable = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let (Some(port), Some(workspace), Some(control), Ok(token)) = (
        port,
        variable("OPENHUMAN_WORKSPACE"),
        variable("CONTROL"),
        std::env::var("OPENHUMAN_CORE_TOKEN"),
    ) else {
        return ExitCode::from(2);
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build();
    let Ok(runtime) = runtime else {
        return ExitCode::FAILURE;
    };
    runtime.block_on(async move {
        let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await else {
            return ExitCode::FAILURE;
        };
        let setup = surface::Setup {
            token,
            workspace,
            control: control.clone(),
            pid: std::process::id(),
        };
        let _fake = surface::Fake::on(listener, setup, port);
        loop {
            let request = control.join("escape");
            if std::fs::remove_file(&request).is_ok()
                && let Ok(program) = std::env::current_exe()
            {
                let _ = std::process::Command::new(program)
                    .arg("escapee")
                    .arg(control.join("escapee"))
                    .spawn();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

/// Leave the process group, report, and stay two minutes at most.
fn escapee(report: &Path) -> ExitCode {
    if rustix::process::setsid().is_err() {
        return ExitCode::FAILURE;
    }
    let partial = report.with_extension("partial");
    let written = std::fs::write(&partial, std::process::id().to_string())
        .and_then(|()| std::fs::rename(&partial, report));
    if written.is_err() {
        return ExitCode::FAILURE;
    }
    std::thread::sleep(Duration::from_secs(120));
    ExitCode::SUCCESS
}
