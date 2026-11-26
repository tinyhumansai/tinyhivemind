//! Every error reads as a lowercase sentence naming what failed.
use super::Error;

#[test]
fn errors_name_what_failed() {
    let cases = [
        (
            Error::Listen(std::io::Error::other("no socket")),
            "cannot listen on loopback: no socket",
        ),
        (
            Error::Random("no entropy".into()),
            "no randomness for a bearer: no entropy",
        ),
        (
            Error::Stopped("panicked".into()),
            "the server task failed: panicked",
        ),
        (
            Error::UnsupportedSchema {
                tool: "t".into(),
                keyword: "pattern".into(),
            },
            "tool t declares the schema keyword pattern, which this server cannot check",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}
