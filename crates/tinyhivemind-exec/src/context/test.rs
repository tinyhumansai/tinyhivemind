//! Secret resolution from the process environment.
use super::{EnvSecrets, Secrets};
use crate::Error;
use tinyhivemind_lang::SecretRef;

#[test]
fn env_secrets_resolve_a_variable_the_process_has() {
    let expected = std::env::var("PATH").expect("the test process has a PATH");
    let resolved = EnvSecrets.resolve(&SecretRef::Env("PATH".into()));
    assert_eq!(resolved.expect("PATH resolves"), expected);
}

#[test]
fn env_secrets_refuse_an_unset_variable() {
    let reference = SecretRef::Env("THM_EXEC_TEST_SURELY_UNSET_VARIABLE".into());
    let error = EnvSecrets.resolve(&reference).expect_err("unset");
    assert!(matches!(error, Error::UnresolvedSecret { reference: r } if r == reference));
    assert_eq!(
        error.to_string(),
        "secret env:THM_EXEC_TEST_SURELY_UNSET_VARIABLE could not be resolved"
    );
}

#[test]
fn env_secrets_refuse_a_store_reference() {
    let reference = SecretRef::Store("github".into());
    let error = EnvSecrets.resolve(&reference).expect_err("no store");
    assert!(matches!(error, Error::UnresolvedSecret { .. }));
    assert_eq!(error.to_string(), "secret store:github could not be resolved");
}
