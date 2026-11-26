//! How this crate's errors reach the port.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.
use super::*;
use tinyhivemind_host::harness::Error as Port;

#[test]
fn errors_keep_their_meaning_at_the_port() {
    let refused: Port = Error::Refused(Refusal::NoModel("s".into())).into();
    assert!(matches!(refused, Port::Refused(_)));
    let resume: Port = Error::Resume {
        session: "x".into(),
        reason: "gone".into(),
    }
    .into();
    assert!(matches!(resume, Port::Resume { ref session_id, .. } if session_id == "x"));
    let closed: Port = Error::Closed.into();
    assert_eq!(closed.to_string(), "harness failed: the seat is stopped");
    let io = Error::io("a file")(std::io::Error::other("full"));
    assert_eq!(io.to_string(), "a file: full");
}
