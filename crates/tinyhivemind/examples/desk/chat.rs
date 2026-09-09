//! A tool-less completion, used when a seat has to speak and must not work.
//!
//! The agent CLI cannot be told to stop calling tools — asked politely, in as
//! many words, it answers by running eight more commands and hitting the
//! deadline again. So the wrap-up does not go through the CLI at all: it is one
//! plain chat completion, where there is no tool to call. Same router, same
//! model. The room's standing account is folded through this too, for the same
//! reason: a fold must not be able to run a command, write a file, or take a
//! turn.
//!
//! The provider layer is [`tinyinference`], which is a dev-dependency of this
//! example and of nothing else. It replaced a `curl` subprocess with a
//! hand-written request body; what that bought is not ergonomics but
//! [`Outcome`] — a refusal, a timeout and an unhealthy upstream are three
//! different things here, and the turn ladder above can only choose a rung if
//! it can tell them apart.

use std::time::Duration;

use tinyinference::{
    failure::{ProviderFailureClass, classify_provider_error},
    message::Message,
    model::{ChatModel, ModelRequest},
    providers::openai::OpenAiModel,
};

/// What one completion produced, or why it produced nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    /// The model answered.
    Answered(String),
    /// The call succeeded and the answer was empty.
    ///
    /// Kept distinct from a failure because it is a different defect with a
    /// different fix: a model that answers nothing has usually spent its
    /// budget somewhere the response does not show.
    Empty,
    /// The call did not finish inside its wall-clock budget.
    TimedOut,
    /// The provider answered with a failure, classified.
    Failed {
        /// Whether trying again could plausibly work.
        class: ProviderFailureClass,
        /// What it said, trimmed to something printable.
        detail: String,
    },
}

impl Outcome {
    /// The text, or the empty string for anything that is not an answer.
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Answered(text) => text,
            _ => "",
        }
    }

    /// Whether trying the same call again is worth a second wall-clock budget.
    pub(crate) const fn worth_retrying(&self) -> bool {
        matches!(
            self,
            Self::TimedOut
                | Self::Failed {
                    class: ProviderFailureClass::Retryable
                        | ProviderFailureClass::RateLimited
                        | ProviderFailureClass::UpstreamUnhealthy,
                    ..
                }
        )
    }
}

/// One model, behind a wall-clock budget.
///
/// The model is boxed rather than named so that a test can drive this against
/// `tinyinference`'s deterministic mock without a network: the wrap-up rung's
/// behavior is the part worth testing, and it is the same behavior whichever
/// provider is underneath.
pub(crate) struct Chat {
    model: Box<dyn ChatModel<()>>,
    timeout: Duration,
}

impl Chat {
    /// Build a client against a router root, without a trailing slash.
    pub(crate) fn new(base: &str, key: &str, model: &str, timeout: Duration) -> Self {
        Self::with_model(
            OpenAiModel::new(key)
                .with_model(model)
                .with_provider("ladder")
                .with_base_url(format!("{}/v1", base.trim_end_matches('/'))),
            timeout,
        )
    }

    /// Build one against any provider, which is how the tests reach it.
    pub(crate) fn with_model(model: impl ChatModel<()> + 'static, timeout: Duration) -> Self {
        Self {
            model: Box::new(model),
            timeout,
        }
    }

    /// Complete one prompt.
    ///
    /// No `max_tokens`, deliberately. A reasoning model spends that budget on
    /// reasoning tokens first: at 1200 the whole cap went to reasoning and the
    /// response came back `finish_reason: length` with `content` empty — a
    /// silent seat that looked like a refusal and was a cap set too low. 8000
    /// bought 21k characters of reasoning and still no answer, on both DeepSeek
    /// tiers. Uncapped, the same call answers in 350 tokens. The deadline here
    /// is wall clock, which is a bound on the *call* rather than a bound on the
    /// thinking inside it.
    pub(crate) async fn complete(&self, prompt: &str) -> Outcome {
        let request = ModelRequest::new(vec![Message::user(prompt)]);
        let call = self.model.invoke(&(), request);
        let answered = match tokio::time::timeout(self.timeout, call).await {
            Err(_) => {
                eprintln!("   [chat] no answer inside {:?}", self.timeout);
                return Outcome::TimedOut;
            }
            Ok(answered) => answered,
        };
        match answered {
            Ok(response) => {
                let text = response.text();
                if text.trim().is_empty() {
                    eprintln!("   [chat] the call succeeded and the answer was empty");
                    return Outcome::Empty;
                }
                Outcome::Answered(text)
            }
            Err(error) => {
                let class = failure_class(&error);
                let detail = error.to_string();
                eprintln!(
                    "   [chat] {class:?}: {}",
                    detail.chars().take(180).collect::<String>()
                );
                Outcome::Failed { class, detail }
            }
        }
    }
}

/// How to read one provider error, when it carries a classification at all.
///
/// An error that is not a provider failure — a request this host built wrong,
/// a response it could not decode — is never worth retrying, so it classifies
/// as non-retryable rather than being guessed at.
fn failure_class(error: &tinyinference::Error) -> ProviderFailureClass {
    match error {
        tinyinference::Error::Provider(provider) => classify_provider_error(provider),
        _ => ProviderFailureClass::NonRetryable,
    }
}

#[cfg(test)]
mod test;
