//! Spawning, stdin, stdout, exit status and the deadline.
use super::support::{FixedSecrets, context, tool};
use crate::{Error, Output, run};
use serde_json::json;
use std::time::{Duration, Instant};

#[tokio::test]
async fn a_package_program_receives_its_substituted_arguments() {
    let tool = tool(
        &["tools/echo_args.sh", "--fixed"],
        json!({"argv": ["{pattern}", "{path?}", "{count}"], "stdin": "none"}),
    );
    let args = json!({"pattern": "fn main", "count": 2});
    let output = run(&tool, &context(), &FixedSecrets, &args, "c1")
        .await
        .expect("ran");
    assert_eq!(
        output,
        Output {
            text: "--fixed\nfn main\n2\n".into(),
            json: None,
            truncated: false
        }
    );
}

#[tokio::test]
async fn stdin_json_carries_the_argument_object() {
    let tool = tool(&["tools/echo_stdin.sh"], json!({}));
    let args = json!({"pattern": "x", "nested": {"n": [1, 2]}});
    let output = run(&tool, &context(), &FixedSecrets, &args, "c1")
        .await
        .expect("ran");
    let echoed: serde_json::Value = serde_json::from_str(&output.text).expect("json");
    assert_eq!(echoed, args);
}

#[tokio::test]
async fn stdin_none_gives_the_program_an_empty_stdin() {
    let tool = tool(&["tools/echo_stdin.sh"], json!({"stdin": "none"}));
    let output = run(&tool, &context(), &FixedSecrets, &json!({"a": 1}), "c1")
        .await
        .expect("ran");
    assert_eq!(output.text, "");
}

#[tokio::test]
async fn json_output_is_parsed() {
    let tool = tool(&["tools/json_ok.sh"], json!({"output": "json"}));
    let output = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
        .await
        .expect("ran");
    assert_eq!(output.json, Some(json!({"ok": true, "n": 2})));
    assert_eq!(output.text, "{\"ok\": true, \"n\": 2}\n");
}

#[tokio::test]
async fn json_output_that_does_not_parse_fails_the_call() {
    let tool = tool(&["tools/not_json.sh"], json!({"output": "json"}));
    let error = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
        .await
        .expect_err("not json");
    assert!(matches!(error, Error::InvalidOutput { .. }), "{error:?}");
    assert!(
        error
            .to_string()
            .starts_with("probe printed invalid json: ")
    );
}

#[tokio::test]
async fn a_non_zero_exit_fails_with_the_code_and_the_stderr_tail() {
    let tool = tool(&["tools/fail.sh"], json!({}));
    let error = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
        .await
        .expect_err("exit 3");
    let Error::Exit { tool, code, stderr } = &error else {
        panic!("expected an exit error, got {error:?}");
    };
    assert_eq!((tool.as_str(), *code), ("probe", Some(3)));
    assert_eq!(stderr.len(), 4096);
    assert!(stderr.ends_with("TAIL-MARKER"));
    assert!(
        error
            .to_string()
            .starts_with("probe exited with status 3: xxx")
    );
}

#[tokio::test]
async fn a_timeout_kills_the_whole_process_group() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pid_file = dir.path().join("child.pid");
    let tool = tool(
        &["tools/sleeper.sh"],
        json!({"argv": ["{pid_file}"], "timeout_ms": 300, "stdin": "none"}),
    );
    let args = json!({"pid_file": pid_file});
    let started = Instant::now();
    let error = run(&tool, &context(), &FixedSecrets, &args, "c1")
        .await
        .expect_err("timed out");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert!(
        matches!(
            error,
            Error::Timeout {
                timeout_ms: 300,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "probe timed out after 300 ms");
    let child = std::fs::read_to_string(&pid_file).expect("the child pid");
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(&child) {
        assert!(
            Instant::now() < deadline,
            "child {child} outlived the timeout"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn alive(pid: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.split_whitespace()
            .nth(2)
            .is_none_or(|state| state != "Z")
    })
}

#[tokio::test]
async fn stdout_past_one_mebibyte_is_truncated_and_says_so() {
    let tool = tool(&["tools/flood.sh"], json!({"stdin": "none"}));
    let output = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
        .await
        .expect("ran");
    assert_eq!(output.text.len(), 1024 * 1024);
    assert!(output.truncated);
}

#[tokio::test]
async fn an_unknown_program_fails_the_call() {
    for program in ["thm-exec-no-such-program-anywhere", "tools/missing.sh"] {
        let tool = tool(&[program], json!({}));
        let error = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
            .await
            .expect_err("no such program");
        assert!(
            matches!(&error, Error::Spawn { program: p, .. } if p == program),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with(&format!("could not start {program}: "))
        );
    }
}

#[tokio::test]
async fn an_empty_command_fails_the_call() {
    let tool = tool(&[], json!({}));
    let error = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
        .await
        .expect_err("no program");
    assert!(matches!(error, Error::EmptyCommand { .. }));
    assert_eq!(error.to_string(), "probe declares no program");
}

#[tokio::test]
async fn an_edited_script_takes_effect_on_the_next_call() {
    let package = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(package.path().join("tools")).expect("tools dir");
    let script = package.path().join("tools/edit.sh");
    let write = |body: &str| {
        std::fs::write(&script, format!("#!/bin/sh\nprintf '{body}'\n")).expect("script");
        let mut permissions = std::fs::metadata(&script).expect("meta").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod");
    };
    let mut context = context();
    context.package_root = package.path().to_path_buf();
    let tool = tool(&["tools/edit.sh"], json!({"stdin": "none"}));
    write("first");
    let first = run(&tool, &context, &FixedSecrets, &json!({}), "c1")
        .await
        .expect("ran");
    write("second");
    let second = run(&tool, &context, &FixedSecrets, &json!({}), "c2")
        .await
        .expect("ran");
    assert_eq!(
        (first.text.as_str(), second.text.as_str()),
        ("first", "second")
    );
}
