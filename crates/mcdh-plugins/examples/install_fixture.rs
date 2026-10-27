//! Install only a caller-provided local test fixture into an explicitly isolated data directory.
use mcdh_core::LocalIndex;
use mcdh_plugins::PluginService;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let data = PathBuf::from(args.next().ok_or("expected isolated data directory")?);
    let archive = PathBuf::from(args.next().ok_or("expected fixture archive")?);
    if !data.is_absolute() || !archive.is_absolute() {
        return Err("fixture paths must be absolute".into());
    }
    let index = LocalIndex::open(data.join("mcdh.db"))?;
    let service = PluginService::new(index)?;
    let preview = service.preview_archive("mcdh-examples/hello", &archive)?;
    let installed = service.install(&preview.token, &preview.fingerprint)?;
    println!("{}", serde_json::to_string_pretty(&installed)?);
    Ok(())
}
