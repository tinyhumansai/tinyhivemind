//! How each failure reaches the port.
use super::Error;
use crate::capability::Refusal;
use tinyhivemind_host::harness::{self, Mismatch};

#[test]
fn a_refusal_stays_typed_through_the_port() {
    let error: harness::Error = Error::from(Refusal::Budget).into();
    let harness::Error::Refused(refusal) = error else {
        unreachable!("refused")
    };
    assert_eq!(refusal.downcast_ref::<Refusal>(), Some(&Refusal::Budget));
}

#[test]
fn supervision_mismatch_and_resume_keep_the_ports_own_variants() {
    let process: harness::Error = Error::from(tinyhivemind_host::process::Error::Unproven).into();
    assert!(matches!(process, harness::Error::Process(_)));
    let mismatch = Mismatch::Pid {
        expected: 1,
        found: 2,
    };
    let mismatch: harness::Error = Error::from(mismatch).into();
    assert!(matches!(
        mismatch,
        harness::Error::Mismatch(Mismatch::Pid { .. })
    ));
    let resume: harness::Error = Error::Resume("thm-scout".into()).into();
    let harness::Error::Resume { session_id, reason } = resume else {
        unreachable!("a resume failure")
    };
    assert_eq!(session_id, "thm-scout");
    assert_eq!(reason, "the instance has no thread thm-scout");
}

#[test]
fn everything_else_is_a_harness_failure_with_its_message() {
    let cases = [
        (
            Error::ToolsNotListed {
                server: "hivemind".into(),
                missing: vec!["hivemind_ask".into()],
            },
            "the instance did not list [\"hivemind_ask\"] of server hivemind",
        ),
        (
            Error::io("the config")(std::io::Error::other("disk full")),
            "the config: disk full",
        ),
        (
            Error::FailedWithoutSession("chat_error".into()),
            "the turn failed before its thread had a transcript: chat_error",
        ),
        (
            Error::Render("bad request".into()),
            "the turn could not be rendered: bad request",
        ),
        (
            Error::from(crate::turn::Error::StreamEnded),
            "the event stream ended before the turn did",
        ),
        (
            Error::from(crate::rpc::Error::Decode("eof".into())),
            "the instance answered with something unreadable: eof",
        ),
    ];
    for (error, message) in cases {
        let error: harness::Error = error.into();
        assert_eq!(error.to_string(), format!("harness failed: {message}"));
    }
}
