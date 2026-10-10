//! Run-scoped namespaces match the existing SWE hive memory layout.
use tinyhivemind_lang::{
    EntryKind, MemoryIdentity, MemoryKind, MemoryLifecycle, Package, RecallMoment, Role, Seat,
    SeatMemory, lower,
};

#[test]
fn swe_run_memory_lowers_seat_ids_and_run_root_without_renaming()
-> Result<(), Box<dyn std::error::Error>> {
    let mut package = Package::default();
    package.memory.root = "team:trial-7-a".into();
    package.manifest.roles.push(Role {
        id: "worker".into(),
        charter: "roles/worker.md".into(),
    });
    package
        .documents
        .insert("roles/worker.md".into(), "Solve the task".into());
    for id in ["lead", "implementer", "tester", "reviewer"] {
        let prompt = format!("seats/{id}.md");
        package
            .documents
            .insert(prompt.clone(), "Work with evidence".into());
        package.manifest.seats.push(Seat {
            id: id.into(),
            label: id.into(),
            role: "worker".into(),
            prompt,
            template: "swe".into(),
            ..Seat::default()
        });
        package.memory.identities.push(MemoryIdentity {
            id: id.into(),
            namespace: format!("team:trial-7-a/agent:{id}"),
            kind: MemoryKind::Agent,
            lifecycle: MemoryLifecycle::Run,
            read_only: false,
            fork_parent: None,
        });
        package.memory.bindings.push(SeatMemory {
            seat: id.into(),
            identity: id.into(),
            reads: vec![],
            recall_at: vec![
                RecallMoment::SessionStart,
                RecallMoment::Rejoin,
                RecallMoment::Compaction,
            ],
            budget_chars: 3000,
            remember: vec![
                EntryKind::Observation,
                EntryKind::FailedAttempt,
                EntryKind::Outcome,
                EntryKind::Note,
            ],
        });
    }
    let wire = serde_json::to_value(&package)?;
    let restored: Package = serde_json::from_value(wire)?;
    assert_eq!(restored, package);
    let lowered = lower(&restored)?;
    for seat in lowered.seats {
        let binding = seat.memory.ok_or("expected run memory binding")?;
        assert_eq!(binding.agent_id, seat.id);
        assert_eq!(binding.root, "team:trial-7-a");
        assert_eq!(
            binding.namespace,
            format!("team:trial-7-a/agent:{}", seat.id)
        );
        assert_eq!(binding.identity.lifecycle, MemoryLifecycle::Run);
        assert_eq!(binding.settings.budget_chars, 3000);
    }
    Ok(())
}
