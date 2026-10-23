//! Check a hive package, show what each seat runs, call one of its tools
//! directly, or deploy it on `OpenHuman` and send it one message.
//!
//! ```sh
//! cargo run -p tinyhivemind-openhuman --example run_package -- <dir>
//! cargo run -p tinyhivemind-openhuman --example run_package -- <dir> --seat scout \
//!     --call word_count --args '{"text":"one two"}'
//! cargo run -p tinyhivemind-openhuman --example run_package -- <dir> --live "Draft the notes"
//! ```
//!
//! `--call` runs the tool exactly as a seat's turn would — the seat's
//! `pre_tool_use` hooks first, then the program — without starting a model
//! or the harness, so editing a script and calling it again is the whole
//! iteration loop. `--live` needs the package's provider credential.
use std::{path::PathBuf, sync::Arc};
use tinyhivemind_exec::{EnvSecrets, ExecContext, HookRunner, Verdict};
use tinyhivemind_openhuman::language::PackageSource;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// What the command line asked for.
#[derive(Debug, Default, PartialEq)]
struct Command {
    dir: PathBuf,
    seat: Option<String>,
    call: Option<String>,
    args: Option<String>,
    live: Option<String>,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Command> {
    let mut command = Command {
        dir: args
            .next()
            .ok_or(
                "usage: run_package <dir> [--seat id] [--call tool --args json] [--live message]",
            )?
            .into(),
        ..Command::default()
    };
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--seat" => command.seat = Some(value),
            "--call" => command.call = Some(value),
            "--args" => command.args = Some(value),
            "--live" => command.live = Some(value),
            other => return Err(format!("unknown flag {other}").into()),
        }
    }
    Ok(command)
}

/// One line per seat: what it runs with.
fn summary(source: &PackageSource) -> String {
    let hive = source.lowered();
    let mut out = format!(
        "hive {} ({}): {} seats\n",
        hive.id,
        hive.name,
        hive.seats.len()
    );
    for seat in &hive.seats {
        let i = &seat.integration;
        let ids = |items: Vec<&str>| {
            if items.is_empty() {
                "-".to_owned()
            } else {
                items.join(",")
            }
        };
        out.push_str(&format!(
            "  {} model={} tools={} hooks={} mcp={} skills={} permissions={}\n",
            seat.id,
            i.model.as_ref().map_or("-", |m| m.model.name.as_str()),
            ids(i.programs.iter().map(|t| t.id.as_str()).collect()),
            ids(i.hooks.iter().map(|h| h.hook.id.as_str()).collect()),
            ids(i.mcp.iter().map(|s| s.id.as_str()).collect()),
            ids(i.skills.iter().map(|s| s.id.as_str()).collect()),
            ids(i.permissions.iter().map(|p| p.id.as_str()).collect()),
        ));
    }
    out
}

/// Run `tool` as `seat` would: its hooks, then the program.
async fn call(
    source: &PackageSource,
    seat: &str,
    tool: &str,
    args: &serde_json::Value,
) -> Result<String> {
    let hive = source.lowered();
    let lowered = hive
        .seats
        .iter()
        .find(|candidate| candidate.id == seat)
        .ok_or_else(|| format!("no seat {seat}"))?;
    let program = lowered
        .integration
        .programs
        .iter()
        .find(|candidate| candidate.id == tool)
        .ok_or_else(|| format!("seat {seat} has no tool {tool}"))?;
    let context = Arc::new(ExecContext {
        package_root: source.root().to_path_buf(),
        workspace: None,
        hive: hive.id.clone(),
        seat: seat.into(),
        features: hive.integrations.features.clone(),
    });
    let hooks = HookRunner::new(
        lowered.integration.hooks.clone(),
        context.clone(),
        Arc::new(EnvSecrets),
    );
    if let Verdict::Deny(message) = hooks.pre_tool_use(tool, args).await {
        return Ok(format!("refused by hook: {message}"));
    }
    let output = tinyhivemind_exec::run(program, &context, &EnvSecrets, args, "cli-1").await?;
    hooks.post_tool_use(tool, args, &output.text).await;
    Ok(output.text)
}

/// Deploy the package and send its hive one message from the host.
async fn live(source: PackageSource, message: &str) -> Result<()> {
    use tinyhivemind_hives::{Destination, SendMessage};
    use tinyhivemind_openhuman::deploy::{BuildOptions, EnvironmentSecrets, HiveDeployment};
    let hive = source.lowered().id.clone();
    let deployment = Box::pin(HiveDeployment::from_packages(
        vec![source],
        Arc::new(EnvironmentSecrets),
        BuildOptions::default(),
    ))
    .await?;
    let coordinator = deployment.host().coordinator();
    coordinator
        .send_as_host(SendMessage {
            message_id: "run-package".into(),
            sender: String::new(),
            destination: Destination::Hive(hive),
            body: message.into(),
            thread: None,
            only_for: Vec::new(),
            starters: Vec::new(),
        })
        .await?;
    let report = coordinator.run_until_idle().await?;
    for row in coordinator.read_transcript(None)? {
        println!("{} → {:?}: {}", row.sender, row.destination, row.body);
    }
    println!("{report:?}");
    Ok(())
}

async fn main_with(command: Command) -> Result<String> {
    let source = PackageSource::load(&command.dir)?;
    if let Some(tool) = &command.call {
        let seat = command
            .seat
            .clone()
            .or_else(|| source.lowered().seats.first().map(|s| s.id.clone()))
            .ok_or("the package has no seats")?;
        let args = serde_json::from_str(command.args.as_deref().unwrap_or("{}"))?;
        return call(&source, &seat, tool, &args).await;
    }
    if let Some(message) = &command.live {
        Box::pin(live(source, message)).await?;
        return Ok(String::new());
    }
    Ok(summary(&source))
}

fn main() -> Result<()> {
    let command = parse(std::env::args().skip(1))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .build()?;
    let output = runtime.block_on(async { tokio::spawn(Box::pin(main_with(command))).await? })?;
    print!("{output}");
    Ok(())
}

#[cfg(test)]
#[path = "run_package/test.rs"]
mod test;
