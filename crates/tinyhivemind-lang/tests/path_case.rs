//! Valid Markdown suffix casing does not restrict edit authority.
use tinyhivemind_lang::{
    Package, Role,
    patch::{PatchClass, apply, diff},
};

#[test]
fn valid_uppercase_markdown_documents_remain_editable() -> Result<(), Box<dyn std::error::Error>> {
    let mut original = Package::default();
    original.constitution.permitted_edits =
        vec![PatchClass::Prompt, PatchClass::Context, PatchClass::Roster];
    original.manifest.roles.push(Role {
        id: "editor".into(),
        charter: "roles/editor.MD".into(),
    });
    original
        .documents
        .insert("roles/editor.MD".into(), "Original\n".into());
    original.manifest.context.push("context/notes.MD".into());
    original.documents.insert(
        "context/notes.MD".into(),
        "<!-- item:note -->\nOriginal\n".into(),
    );
    let mut prompt = original.clone();
    prompt
        .documents
        .insert("roles/editor.MD".into(), "Improved\n".into());
    let mut context = original.clone();
    context.documents.insert(
        "context/notes.MD".into(),
        "<!-- item:note -->\nImproved\n".into(),
    );
    let mut roster = original.clone();
    roster.manifest.roles.push(Role {
        id: "new".into(),
        charter: "roles/new.MD".into(),
    });
    roster
        .documents
        .insert("roles/new.MD".into(), "New role\n".into());
    for (candidate, class) in [
        (prompt, PatchClass::Prompt),
        (context, PatchClass::Context),
        (roster, PatchClass::Roster),
    ] {
        let patch = diff(&original, &candidate, class)?;
        assert_eq!(apply(&original, &patch)?.candidate, candidate);
    }
    Ok(())
}
