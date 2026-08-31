//! Unit tests for the `Greet` payloads.
//!
//! These pin the serde representation. It is the wire form: a host and a module
//! that disagree about a field name fail at runtime with a decode error, so the
//! shape is asserted here rather than assumed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{GreetRequest, GreetResponse};

#[test]
fn a_request_serializes_to_its_wire_form() {
    let encoded = serde_json::to_value(GreetRequest::new("Ferris")).unwrap();
    assert_eq!(encoded, serde_json::json!({ "name": "Ferris" }));
}

#[test]
fn a_response_serializes_to_its_wire_form() {
    let encoded = serde_json::to_value(GreetResponse::new("Hello, Ferris!")).unwrap();
    assert_eq!(encoded, serde_json::json!({ "greeting": "Hello, Ferris!" }));
}

#[test]
fn a_request_round_trips_through_json() {
    let request = GreetRequest::new("  Ferris  ");
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<GreetRequest>(&encoded).unwrap(),
        request
    );
}

#[test]
fn a_response_round_trips_through_json() {
    let response = GreetResponse::new("Hello, Ferris!");
    let encoded = serde_json::to_string(&response).unwrap();
    assert_eq!(
        serde_json::from_str::<GreetResponse>(&encoded).unwrap(),
        response
    );
}

#[test]
fn a_request_missing_its_name_is_rejected() {
    let decoded = serde_json::from_value::<GreetRequest>(serde_json::json!({}));
    assert!(decoded.is_err());
}

#[test]
fn a_response_missing_its_greeting_is_rejected() {
    let decoded = serde_json::from_value::<GreetResponse>(serde_json::json!({}));
    assert!(decoded.is_err());
}

#[test]
fn constructors_accept_both_borrowed_and_owned_names() {
    assert_eq!(
        GreetRequest::new(String::from("Ferris")),
        GreetRequest::new("Ferris")
    );
    assert_eq!(
        GreetResponse::new(String::from("Hi")),
        GreetResponse::new("Hi")
    );
}
