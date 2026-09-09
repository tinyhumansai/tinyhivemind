//! Unit tests for the tool-less channel, against a deterministic mock model.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use tinyinference::{
    model::{ModelRequest as Request, ModelResponse},
    providers::MockModel,
};

/// A model that never answers, so a wall-clock budget is what decides.
struct NeverAnswers;

#[async_trait::async_trait]
impl ChatModel<()> for NeverAnswers {
    async fn invoke(&self, _: &(), _: Request) -> tinyinference::Result<ModelResponse> {
        std::future::pending().await
    }
}

fn chat(model: impl ChatModel<()> + 'static) -> Chat {
    Chat::with_model(model, Duration::from_secs(30))
}

#[tokio::test]
async fn answers_from_the_model_it_was_built_against() {
    let outcome = chat(MockModel::echo()).complete("B holds at 10^18").await;
    assert_eq!(outcome, Outcome::Answered("B holds at 10^18".into()));
    assert_eq!(outcome.text(), "B holds at 10^18");
    assert!(
        !outcome.worth_retrying(),
        "an answer is not something to ask for twice",
    );
}

#[tokio::test]
async fn an_empty_answer_is_not_a_failure_and_is_not_retried() {
    let outcome = chat(MockModel::constant("   "))
        .complete("say something")
        .await;
    assert_eq!(
        outcome,
        Outcome::Empty,
        "a call that succeeded and said nothing is its own defect",
    );
    assert_eq!(outcome.text(), "");
    assert!(!outcome.worth_retrying());
}

#[tokio::test]
async fn a_call_that_outruns_its_budget_is_a_timeout_worth_retrying() {
    let outcome = Chat::with_model(NeverAnswers, Duration::from_millis(20))
        .complete("anything")
        .await;
    assert_eq!(outcome, Outcome::TimedOut);
    assert!(
        outcome.worth_retrying(),
        "a deadline says nothing about whether the model would have answered",
    );
}

#[test]
fn only_a_failure_the_provider_called_transient_is_retried() {
    let retryable = [
        ProviderFailureClass::Retryable,
        ProviderFailureClass::RateLimited,
        ProviderFailureClass::UpstreamUnhealthy,
    ];
    for class in retryable {
        assert!(
            Outcome::Failed {
                class,
                detail: String::new()
            }
            .worth_retrying(),
            "{class:?}",
        );
    }
    for class in [
        ProviderFailureClass::NonRetryable,
        ProviderFailureClass::NonRetryableRateLimit,
    ] {
        assert!(
            !Outcome::Failed {
                class,
                detail: String::new()
            }
            .worth_retrying(),
            "{class:?} answered twice is the same refusal twice",
        );
    }
}

#[test]
fn an_error_this_host_caused_is_never_worth_retrying() {
    assert_eq!(
        failure_class(&tinyinference::Error::Validation("no messages".into())),
        ProviderFailureClass::NonRetryable,
        "a request built wrong is built wrong the second time too",
    );
}
