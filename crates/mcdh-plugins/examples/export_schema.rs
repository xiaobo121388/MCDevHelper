fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("expected output JSON Schema path")?;
    let mut schema = serde_json::to_value(schemars::schema_for!(mcdh_plugins::PluginManifest))?;
    schema["properties"]["schema_version"]["const"] = serde_json::json!(1);
    schema["properties"]["api_version"]["const"] = serde_json::json!(mcdh_plugins::API_VERSION);
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&schema)?),
    )?;
    Ok(())
}
