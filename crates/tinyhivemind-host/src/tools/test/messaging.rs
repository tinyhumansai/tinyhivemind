//! Reads and sends are the bound caller's own.
use super::support::{call, hive, with_agents};
use crate::{
    Error, Result,
    tools::{Belt, Kind},
};
use serde_json::json;

#[tokio::test]
async fn direct_replies_are_read_back_by_the_caller_alone() -> Result<()> {
    let coordinator = with_agents(&["a", "b", "outsider"]).await?;
    let belt = Belt::new(&coordinator);
    let receipt = call(
        belt,
        "a",
        Kind::SendAgent,
        json!({"agent_id":"b","message_id":"q","body":"question"}),
    )
    .await?;
    assert_eq!(coordinator.run_until_idle().await?.completed, 1);
    let rows = call(
        belt,
        "a",
        Kind::Read,
        json!({"agent_id":"b","after":receipt["sequence"]}),
    )
    .await?;
    assert_eq!(rows[0]["body"], "answer");
    assert_eq!(rows[0]["sender"], "b");
    let outsider = call(belt, "outsider", Kind::Read, json!({"agent_id":"b"})).await?;
    assert_eq!(outsider, json!([]));
    assert_eq!(coordinator.run_until_idle().await?.completed, 0);
    Ok(())
}

#[tokio::test]
async fn hives_are_listed_read_and_sent_to_by_their_members() -> Result<()> {
    let coordinator = with_agents(&["a", "b", "outsider"]).await?;
    coordinator.create_hive(hive(&["a", "b"])).await?;
    let belt = Belt::new(&coordinator);
    let hives = call(belt, "a", Kind::ListHives, json!({})).await?;
    assert_eq!(hives[0]["hive_id"], "h");
    assert_eq!(
        call(belt, "outsider", Kind::ListHives, json!({})).await?,
        json!([])
    );
    assert_eq!(
        call(belt, "a", Kind::ListAgents, json!({})).await?,
        json!(["a", "b", "outsider"])
    );
    let receipt = call(
        belt,
        "a",
        Kind::SendHive,
        json!({"hive_id":"h","message_id":"m","body":"visible","only_for":["b"]}),
    )
    .await?;
    assert_eq!(receipt["message_id"], "m");
    let rows = call(belt, "b", Kind::Read, json!({"hive_id":"h"})).await?;
    assert_eq!(rows[0]["sender"], "a");
    assert_eq!(rows[0]["only_for"], json!(["b"]));
    assert!(matches!(
        call(belt, "outsider", Kind::Read, json!({"hive_id":"h"})).await,
        Err(Error::Coordinator(_))
    ));
    Ok(())
}
