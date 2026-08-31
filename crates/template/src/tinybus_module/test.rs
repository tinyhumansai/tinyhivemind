//! Tests for the `TinyBus` module adapter and its declared surface.

use super::{GreetingService, setup};
use template_bus::{GreetRequest, GreetResponse, names};
use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Interface};

#[test]
fn declared_methods_match_the_dispatch_table() {
    let methods = GreetingService
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert_eq!(methods, names::METHODS.to_vec());
}

#[test]
fn the_served_interface_name_matches_the_contract() {
    assert_eq!(GreetingService.name().to_string(), names::INTERFACE);
}

#[tokio::test]
async fn module_serves_greetings_over_a_real_bus() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone()).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let reply: GreetResponse = proxy
        .call(names::methods::GREET, (GreetRequest::new("Ferris"),))
        .await?;

    assert_eq!(reply, GreetResponse::new("Hello, Ferris!"));
    Ok(())
}

#[tokio::test]
async fn module_rejects_an_empty_name_over_the_bus() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone()).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let result = proxy
        .call::<GreetResponse>(names::methods::GREET, (GreetRequest::new("   "),))
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "whitespace-only names unexpectedly succeeded",
        ));
    };
    assert!(error.to_string().contains("name must not be empty"));
    Ok(())
}
