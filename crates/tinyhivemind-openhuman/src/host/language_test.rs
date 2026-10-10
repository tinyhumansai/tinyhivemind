//! Management factories install memory before registration and cannot drop it.
use super::*;
use tinyhivemind_lang::LoweredMemoryBinding;

fn binding() -> LoweredMemoryBinding {
    serde_json::from_value(serde_json::json!({
        "agent_id":"shared", "root":"team:hive", "namespace":"team:hive/agent:shared", "kind":"agent",
        "identity":{"id":"memory","namespace":"team:hive/agent:shared","kind":"agent","lifecycle":"hive","read_only":false,"fork_parent":null},
        "settings":{"seat":"scout","identity":"memory","reads":[],"recall_at":[],"budget_chars":128,"remember":[]}
    })).unwrap()
}
struct BindingFactory {
    agent: Agent,
    observed: Arc<Mutex<Option<LoweredMemoryBinding>>>,
}
impl AgentFactory for BindingFactory {
    fn create(&self, _: String, _: serde_json::Value) -> AgentFuture {
        Box::pin(async { Err(Error::MemoryBindingUnsupported) })
    }
    fn create_with_memory(
        &self,
        _: String,
        _: serde_json::Value,
        memory: Option<LoweredMemoryBinding>,
    ) -> AgentFuture {
        *self.observed.lock().unwrap() = memory;
        let agent = self.agent.clone();
        Box::pin(async move { Ok(agent) })
    }
}
#[test]
fn management_installs_and_verifies_the_requested_memory_before_registration() {
    let _guard = RUNTIME_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    executor().block_on(async {
        tokio::spawn(async {
            let (runtime, _backend, host) = Box::pin(fixture()).await;
            let portable = binding();
            let request = || ManagementRequest::CreateAgent {
                template: "research".into(),
                config: serde_json::json!({}),
                memory: Some(Box::new(portable.clone())),
            };
            let agent = runtime
                .agent(
                    AgentSpec::new("scout")
                        .memory(crate::language::memory_binding(&portable).unwrap()),
                )
                .unwrap();
            let observed = Arc::new(Mutex::new(None));
            let host = host
                .with_management(
                    Arc::new(BindingFactory {
                        agent: agent.clone(),
                        observed: observed.clone(),
                    }),
                    Arc::new(Allow),
                )
                .unwrap();
            assert_eq!(
                host.manage("host", request()).await.unwrap()["agent_id"],
                "scout"
            );
            assert_eq!(*observed.lock().unwrap(), Some(portable.clone()));
            assert_eq!(agent.config().memory.agent_id.as_deref(), Some("shared"));
            assert_eq!(host.coordinator().list_agents().unwrap(), vec!["scout"]);
            drop(host);
        })
        .await
        .unwrap();
    });
}
#[test]
fn legacy_factory_refuses_binding_and_opted_in_factory_cannot_ignore_it() {
    let _guard = RUNTIME_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    executor().block_on(async {
        tokio::spawn(async {
            let (runtime, _backend, host) = Box::pin(fixture()).await;
            let agent = runtime.agent(AgentSpec::new("unbound")).unwrap();
            let request = || ManagementRequest::CreateAgent {
                template: "configured".into(),
                config: serde_json::json!({}),
                memory: Some(Box::new(binding())),
            };
            let legacy = host
                .with_management(Arc::new(ConfiguredFactory(agent.clone())), Arc::new(Allow))
                .unwrap();
            assert!(matches!(
                legacy.manage("host", request()).await,
                Err(Error::MemoryBindingUnsupported)
            ));
            assert_eq!(
                legacy.coordinator().list_agents().unwrap(),
                Vec::<String>::new()
            );
            drop(legacy);
            let host = OpenHumanHost::new(
                runtime.runtime_id().into(),
                Coordinator::new(
                    runtime.runtime_id().into(),
                    Arc::new(MemoryStorage::new()),
                    CoordinatorOptions::default(),
                )
                .await
                .unwrap(),
            )
            .unwrap()
            .with_management(
                Arc::new(BindingFactory {
                    agent,
                    observed: Arc::new(Mutex::new(None)),
                }),
                Arc::new(Allow),
            )
            .unwrap();
            assert!(matches!(
                host.manage("host", request()).await,
                Err(Error::UnboundSeat { .. })
            ));
            assert_eq!(
                host.coordinator().list_agents().unwrap(),
                Vec::<String>::new()
            );
        })
        .await
        .unwrap();
    });
}
