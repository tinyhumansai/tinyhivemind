//! Turn scopes and the attributed prompt.
use super::*;

#[test]
fn turn_scope_names_the_episode_its_thread_and_distinct_senders() {
    use tinyhivemind_hives::{Destination, EpisodeContext, Message, TurnRequest};
    let row = |id: &str, sender: &str, thread| Message {
        scheduled_job_id: None,
        message_id: id.into(),
        sequence: 0,
        sender: sender.into(),
        destination: Destination::Hive("work".into()),
        body: String::new(),
        thread,
        episode_id: Some("episode:0".into()),
        only_for: vec![],
    };
    let episode = EpisodeContext {
        episode_id: "episode:0".into(),
        hive_id: "work".into(),
        thread: Some(7),
        brief: "brief".into(),
    };
    let request = TurnRequest {
        turn_id: String::new(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "a".into(),
        session_id: None,
        messages: vec![
            row("m1", "b", None),
            row("m2", "c", Some(7)),
            row("m3", "b", None),
        ],
        memberships: vec![],
        episode: Some(episode.clone()),
        resumption: None,
    };
    let scope = TurnScope::from_request(&request);
    assert_eq!(scope.agent_id, "a");
    assert_eq!(scope.episode, Some(episode));
    assert_eq!(scope.message_ids, ["m1", "m2", "m3"]);
    assert_eq!(scope.senders, ["b", "c"]);
    assert_eq!(scope.destination, Destination::Hive("work".into()));
    assert_eq!(scope.thread, Some(7));
    let mut direct = request;
    direct.episode = None;
    direct.messages = vec![Message {
        scheduled_job_id: None,
        destination: Destination::Agent("a".into()),
        thread: None,
        ..row("d1", tinyhivemind_hives::HOST_ID, None)
    }];
    let scope = TurnScope::from_request(&direct);
    assert_eq!(scope.destination, Destination::Agent("a".into()));
    assert_eq!(scope.thread, None);
    assert_eq!(scope.senders, [tinyhivemind_hives::HOST_ID]);
    direct.messages.clear();
    assert_eq!(
        TurnScope::from_request(&direct).destination,
        Destination::Agent("a".into())
    );
}

#[test]
fn renders_attribution_and_explicit_episode_without_reseeding() -> tinyhivemind_hives::Result<()> {
    let request = TurnRequest {
        turn_id: String::new(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "a".into(),
        session_id: Some("history".into()),
        messages: vec![],
        memberships: vec![],
        episode: None,
        resumption: None,
    };
    let prompt = render(&request)?;
    assert!(prompt.contains("\"agent_id\":\"a\""));
    assert!(prompt.contains("\"session_id\":\"history\""));
    assert!(!prompt.contains("Host resumption note"));
    Ok(())
}
#[test]
fn renders_a_release_note_ahead_of_the_attributed_context() -> tinyhivemind_hives::Result<()> {
    let request = TurnRequest {
        turn_id: String::new(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "a".into(),
        session_id: None,
        messages: vec![],
        memberships: vec![],
        episode: None,
        resumption: Some("approved: staging only".into()),
    };
    let prompt = render(&request)?;
    let note = prompt.find("Host resumption note: approved: staging only");
    assert!(note.is_some());
    assert!(note < prompt.find("Incoming attributed Hivemind context"));
    Ok(())
}
