//! Host log sequence, paging and private audience behavior.
// Test fixture failures should identify invalid data immediately.
#![allow(clippy::unwrap_used)]
use super::*;
#[tokio::test]
async fn stores_and_projects_ordered_private_conversations_and_other_hives() {
    let log = MemoryLog::numbered_from("hive", Sequence(0));
    assert_eq!(log.latest(), None);
    let root = log.append("a", "ask", None, &["b".into(), "b".into()]);
    log.append("b", "answer", Some(root), &[]);
    log.append("operator", "public", None, &[]);
    log.append_to("other", "desk", "system", None, &[]);
    log.append("a", "missing root", Some(Sequence(100)), &[]);
    assert_eq!(log.latest(), Some(Sequence(4)));
    assert_eq!(
        log.desk_since("a", None),
        vec!["@a: ask", "@operator: public"]
    );
    assert_eq!(
        log.desk_since("stranger", Some(root)),
        vec!["@operator: public"]
    );
    assert_eq!(log.thread(root), vec!["@a: ask", "@b: answer"]);
    assert_eq!(log.thread_since(root, Some(root)), vec!["@b: answer"]);
    let page = log.read_before(None, 2).await.unwrap();
    assert_eq!(page.messages.len(), 2);
    assert_eq!(page.next_before, Some(Sequence(3)));
    assert_eq!(page.messages[1].chat_id.as_deref(), Some("other"));
    assert!(matches!(
        page.messages[1].author,
        SessionAuthor::System { .. }
    ));
    let rest = log.read_before(page.next_before, 10).await.unwrap();
    assert_eq!(rest.next_before, None);
    assert!(matches!(rest.messages[0].author, SessionAuthor::Operator));
    assert_eq!(
        rest.messages[1].audience,
        Audience::Aside {
            members: vec!["a".into()]
        }
    );
    assert_eq!(
        rest.messages[2].audience,
        Audience::Aside {
            members: vec!["b".into()]
        }
    );
    assert_eq!(log.all().len(), 5);
    assert_eq!(
        MemoryLog::new("one").append("a", "hi", None, &[]),
        Sequence(1)
    );
}
