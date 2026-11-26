//! Registration completion cannot lose a wakeup when a runner is already claimed.
#[tokio::test]
async fn waiting_claim_wakes_on_registration_and_later_claims_do_not_wait() {
    let activation = std::sync::Arc::new(super::Activation::default());
    let claim = activation.clone();
    let waiting = tokio::spawn(async move {
        claim.wait().await;
    });
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    activation.activate();
    assert!(waiting.await.is_ok());
    activation.wait().await;
    assert!(activation.is_ready());
}
