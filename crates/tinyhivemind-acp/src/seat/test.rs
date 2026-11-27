//! A new session's first turn carries the seat's prompt.
use super::*;

#[test]
fn the_seat_prompt_opens_the_turn() {
    let text = with_instructions("  You are the lead.\n", "Incoming turn");
    assert_eq!(
        text,
        format!("{INSTRUCTIONS}\n\nYou are the lead.\n\nIncoming turn")
    );
}

#[test]
fn a_seat_without_a_prompt_gets_the_turn_alone() {
    assert_eq!(with_instructions(" \n", "Incoming turn"), "Incoming turn");
}
