// Linked-path baseline: raw agent turns against the spike fixture.
use openhuman_embed::{Access, AgentSpec, Provider, Runtime, Workspace};
use std::time::Instant;
struct Post;
#[async_trait::async_trait]
impl tinytools::Tool for Post {
    fn name(&self) -> &'static str { "hivemind_post" }
    fn description(&self) -> &'static str { "Post to the episode." }
    fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object","properties":{"body":{"type":"string"}},"required":["body"]}) }
    async fn execute(&self, _: serde_json::Value) -> anyhow::Result<tinytools::ToolResult> { Ok(tinytools::ToolResult::success("posted")) }
}
fn kb(key: &str) -> u64 {
    let f = if key == "Pss:" { "/proc/self/smaps_rollup" } else { "/proc/self/status" };
    std::fs::read_to_string(f).unwrap().lines().find(|l| l.starts_with(key)).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap()
}
async fn run(base: String) -> anyhow::Result<()> {
    let t0 = Instant::now();
    let backend = tinyhivemind_openhuman::offline::backend().await;
    let rt = Box::pin(Runtime::builder().config(tinyhivemind_openhuman::offline::config()).workspace(Workspace::Ephemeral)
        .backend_url(backend.uri()).provider(Provider::openai_compatible(format!("{base}/v1"), "dummy").model("mock"))
        .access(Access::full()).build()).await?;
    let agent = rt.agent(AgentSpec::new("seat").system_prompt("You are a hive seat.").tools(|_| openhuman_embed::HostTurnTools::advertised(vec![Box::new(Post) as Box<dyn openhuman_embed::Tool>])))?;
    println!("boot_ms={} idle_rss_mb={} idle_pss_mb={}", t0.elapsed().as_millis(), kb("VmRSS:")/1024, kb("Pss:")/1024);
    for prompt in ["PLAIN", "CALLTOOL"] {
        let mut v = vec![];
        for i in 0..8 {
            let t = Instant::now();
            let r = agent.turn(format!("{prompt} {i}")).session("thread").send().await?;
            let us = t.elapsed().as_micros();
            if i == 0 { println!("{prompt} first_us={us} reply={:?}", r.reply); } else if i > 2 { v.push(us); }
        }
        v.sort(); println!("{prompt} median_us={} all={v:?}", v[v.len()/2]);
    }
    println!("after_rss_mb={} after_pss_mb={}", kb("VmRSS:")/1024, kb("Pss:")/1024);
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let base = std::env::args().nth(1).unwrap();
    tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(16<<20).build()?.block_on(async { tokio::spawn(run(base)).await? })
}
