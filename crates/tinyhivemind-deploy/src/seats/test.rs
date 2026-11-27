//! Starting and stopping seats without a real agent: refusals before
//! anything starts, cleanup after a start that fails partway, and stopping.
#![allow(clippy::unwrap_used, clippy::panic)] // Tests fail loudly on unexpected results.
use super::*;
use crate::Pinned;
use crate::fixture::seat;
use tinyhivemind_hives::{CoordinatorOptions, MemoryStorage};
use tinyhivemind_lang::HarnessKind;

async fn coordinator() -> Coordinator {
    Coordinator::new(
        "runtime".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await
    .unwrap()
}

fn on(id: &str, kind: HarnessKind, root: &std::path::Path) -> ProcessSeat {
    let seat = seat(id, kind, None, "http://127.0.0.1:9/v1", "mock");
    ProcessSeat::new(seat, root.join("seats").join(id))
}

async fn start(
    seats: Vec<ProcessSeat>,
    installs: &Installs,
    root: &std::path::Path,
) -> Result<ProcessSeats> {
    let coordinator = coordinator().await;
    let services = Services::new(coordinator.clone());
    let deployment = Deployment::new(root.join("deployment"));
    ProcessSeats::start(&coordinator, services, seats, installs, &deployment).await
}

#[tokio::test]
async fn no_seats_start_and_stop_and_stopping_again_is_harmless() {
    let root = tempfile::tempdir().unwrap();
    let mut running = start(Vec::new(), &Installs::default(), root.path())
        .await
        .unwrap();
    assert_eq!(running.seats(), Vec::<&str>::new());
    assert!(format!("{running:?}").contains("ProcessSeats"));
    running.stop().await.unwrap();
    running.stop().await.unwrap();
}

#[tokio::test]
async fn a_seat_that_cannot_run_here_starts_nothing() {
    let root = tempfile::tempdir().unwrap();
    let seats = vec![
        on("a", HarnessKind::OpenHuman, root.path()),
        on("b", HarnessKind::Codex, root.path()),
    ];
    let installs = Installs {
        openhuman: Some(Pinned::new("/bin/true", "0.64.15")),
        ..Installs::default()
    };
    let error = start(seats, &installs, root.path()).await.unwrap_err();
    assert!(
        matches!(error, Error::MissingInstall { ref seat, .. } if seat == "b"),
        "{error}"
    );
    // Refused before the deployment's directory, or any seat's, was made.
    assert!(!root.path().join("deployment").exists());
    assert!(!root.path().join("seats").exists());
}

#[tokio::test]
async fn a_seat_that_fails_to_start_is_reported_and_nothing_is_left() {
    let root = tempfile::tempdir().unwrap();
    let installs = Installs {
        openhuman: Some(Pinned::new("/bin/true", "0.64.15")),
        ..Installs::default()
    };
    let seats = vec![on("a", HarnessKind::OpenHuman, root.path())];
    let error = start(seats, &installs, root.path()).await.unwrap_err();
    assert!(
        matches!(error, Error::Start { ref seat, .. } if seat == "a"),
        "{error}"
    );
    assert!(root.path().join("seats/a").is_dir());
    // Every directory the deployment made is private, whatever the umask.
    for made in ["deployment", "seats", "seats/a"] {
        assert_eq!(mode(&root.path().join(made)), 0o700, "{made}");
    }
}

#[tokio::test]
async fn an_existing_deployment_directory_is_made_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().unwrap();
    let deployment = root.path().join("deployment");
    std::fs::create_dir(&deployment).unwrap();
    std::fs::set_permissions(&deployment, std::fs::Permissions::from_mode(0o777)).unwrap();
    let mut running = start(Vec::new(), &Installs::default(), root.path())
        .await
        .unwrap();
    assert_eq!(mode(&deployment), 0o700);
    running.stop().await.unwrap();
}

/// A harness whose first `stops` stops fail.
struct Stubborn {
    stops: std::sync::Mutex<usize>,
}

impl Harness for Stubborn {
    fn kind(&self) -> HarnessKind {
        HarnessKind::OpenHuman
    }

    fn start(&self, _: SeatPlan, _: ToolEndpoint) -> tinyhivemind_host::harness::StartFuture {
        Box::pin(async { Err(tinyhivemind_host::harness::Error::Harness("no".into())) })
    }

    fn stop(&self, _: &str) -> tinyhivemind_host::harness::StopFuture {
        let mut stops = self.stops.lock().unwrap();
        let failing = *stops > 0;
        *stops = stops.saturating_sub(1);
        Box::pin(async move {
            if failing {
                Err(tinyhivemind_host::harness::Error::Harness(
                    "still running".into(),
                ))
            } else {
                Ok(())
            }
        })
    }
}

#[tokio::test]
async fn a_seat_whose_stop_fails_is_kept_and_stopped_again() {
    let stubborn = |stops| -> Arc<dyn Harness> {
        Arc::new(Stubborn {
            stops: std::sync::Mutex::new(stops),
        })
    };
    let mut running = ProcessSeats {
        server: None,
        gateways: BTreeMap::new(),
        seats: vec![
            ("a".into(), stubborn(1)),
            ("b".into(), stubborn(0)),
            ("c".into(), stubborn(1)),
        ],
    };
    let error = running.stop().await.unwrap_err();
    assert!(
        matches!(error, Error::Stop { ref seat, .. } if seat == "a"),
        "{error}"
    );
    // `b` stopped; the two that failed are still tracked, in order.
    assert_eq!(running.seats(), vec!["a", "c"]);
    running.stop().await.unwrap();
    assert_eq!(running.seats(), Vec::<&str>::new());
}

/// `path`'s permission bits.
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn two_seats_with_one_id_are_refused_by_the_tool_server() {
    let root = tempfile::tempdir().unwrap();
    let installs = Installs {
        openhuman: Some(Pinned::new("/bin/true", "0.64.15")),
        ..Installs::default()
    };
    let seats = vec![
        on("a", HarnessKind::OpenHuman, root.path()),
        on("a", HarnessKind::OpenHuman, root.path()),
    ];
    let error = start(seats, &installs, root.path()).await.unwrap_err();
    assert!(matches!(error, Error::Server(_)), "{error}");
}

#[tokio::test]
async fn a_gateway_that_does_not_start_is_reported_by_name() {
    let root = tempfile::tempdir().unwrap();
    let installs = Installs {
        node: Some("/bin/true".into()),
        openclaw: Some(Pinned::new("/opt/openclaw/openclaw.mjs", "2026.10.33")),
        ..Installs::default()
    };
    let mut deployment = Deployment::new(root.path().join("deployment"));
    deployment.gateway_ready = std::time::Duration::from_secs(10);
    let coordinator = coordinator().await;
    let seats = vec![on("claw", HarnessKind::OpenClaw, root.path())];
    let services = Services::new(coordinator.clone());
    let error = ProcessSeats::start(&coordinator, services, seats, &installs, &deployment)
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Gateway { ref gateway, .. } if gateway == "gateway"),
        "{error}"
    );
    // OpenClaw refuses a temporary directory under a group- or
    // world-writable one, so the Gateway's are private.
    let state = root.path().join("deployment/gateway/state");
    for made in ["openclaw", "home", "workspaces", "workspaces/claw"] {
        assert_eq!(mode(&state.join(made)), 0o700, "{made}");
    }
}

#[test]
fn a_process_seat_takes_the_tools_it_is_given() {
    let root = tempfile::tempdir().unwrap();
    let seat = on("a", HarnessKind::OpenHuman, root.path());
    assert!(format!("{:?}", seat.tools).contains('a'));
    let seat = seat.with_tools(SeatTools::new("a").with_allowed(vec!["hivemind_post".into()]));
    assert!(format!("{:?}", seat.tools).contains("hivemind_post"));
}
