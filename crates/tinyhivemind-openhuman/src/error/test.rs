//! Host failures reach adapter callers as the variants they always matched.
use super::{Error, into_host};
use tinyhivemind_host::{ArgumentError, Error as Host};

#[test]
fn host_failures_keep_their_variant_and_text() {
    for (host, matches) in [
        (
            Host::ManagementDisabled,
            (|e| matches!(e, Error::ManagementDisabled)) as fn(&Error) -> bool,
        ),
        (Host::Unauthorized("actor".into()), |e| {
            matches!(e, Error::Unauthorized(_))
        }),
        (Host::SendDenied("no".into()), |e| {
            matches!(e, Error::SendDenied(_))
        }),
        (Host::Arguments(ArgumentError::OneDestination), |e| {
            matches!(e, Error::Harness(_))
        }),
        (
            Host::Coordinator(tinyhivemind_hives::Error::UnknownAgent("a".into())),
            |e| matches!(e, Error::Coordinator(_)),
        ),
        (
            Host::Json(serde::de::Error::custom("bad")),
            |e| matches!(e, Error::Json(_)),
        ),
        (Host::Host("the host's own words".into()), |e| {
            matches!(e, Error::Harness(_))
        }),
    ] {
        let text = host.to_string();
        let error = Error::from(host);
        assert!(matches(&error), "{error:?}");
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn an_adapter_error_crosses_the_host_belt_unchanged() {
    let crossed = into_host(Error::RuntimeMismatch);
    assert_eq!(crossed.to_string(), Error::RuntimeMismatch.to_string());
    assert!(matches!(Error::from(crossed), Error::RuntimeMismatch));
}
