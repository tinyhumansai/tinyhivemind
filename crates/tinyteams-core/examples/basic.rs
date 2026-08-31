//! Minimal end-to-end usage of the crate.
//!
//! Examples are compiled and linted in CI, so they cannot drift from the API.
//! Run it with:
//!
//! ```sh
//! cargo run -p tinyteams-core --example basic
//! ```

use tinyteams_core::chat::{is_general_chat, same_conversation};
use tinyteams_core::desk::{Desk, DeskMember, DeskOrder, DeskSet};
use tinyteams_core::error::Result;

fn main() -> Result<()> {
    // The default desk has four spellings and one identity.
    for spelling in [None, Some(""), Some("main"), Some("General")] {
        println!(
            "{spelling:?} is the General desk: {}",
            is_general_chat(spelling)
        );
    }

    // So an unaddressed message and a reply journaled under "General" belong to
    // the same transcript, whichever id happened to write them.
    println!(
        "None and Some(\"General\") are one conversation: {}",
        same_conversation(None, Some("General")),
    );

    // Everything else compares verbatim, case included.
    println!(
        "engineering and Engineering are one conversation: {}",
        same_conversation(Some("engineering"), Some("Engineering")),
    );

    let declared = [Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: Some("Build the product".into()),
        members: vec!["alice".into(), "bob".into()],
    }];
    let additions = [DeskMember {
        desk_id: "engineering".into(),
        agent_id: "cara".into(),
    }];
    let orders = [DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["cara".into(), "alice".into(), "bob".into()],
    }];
    let desks = DeskSet::new(&declared, &[], &additions, &orders, &[]);
    println!("Engineering members: {:?}", desks.members("Engineering")?);
    Ok(())
}
