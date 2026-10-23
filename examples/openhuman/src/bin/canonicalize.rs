//! Throwaway: rewrite each package's canonical files in their complete form.
fn main() -> anyhow::Result<()> {
    for name in std::env::args().skip(1) {
        let root = tinyhivemind_openhuman_example::root(&name);
        let package = tinyhivemind_openhuman_example::manifest(&name)?;
        for (file, value) in [
            ("hive.json", serde_json::to_value(&package.manifest)?),
            ("constitution.json", serde_json::to_value(&package.constitution)?),
            ("memory.json", serde_json::to_value(&package.memory)?),
        ] {
            std::fs::write(root.join(file), serde_json::to_string_pretty(&value)? + "\n")?;
        }
    }
    Ok(())
}
