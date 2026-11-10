use super::*;
use serde_json::json;
use std::fs;
use std::io::Write;

fn png_bytes() -> Vec<u8> {
    let mut bytes = vec![];
    {
        let encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[128])
            .unwrap();
    }
    bytes
}

#[test]
fn png_must_decode_and_respect_size_limit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("skin.png");
    let bytes = png_bytes();
    fs::write(&path, &bytes).unwrap();
    assert!(
        preview(&path)
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    fs::write(&path, &bytes[..33]).unwrap();
    assert!(preview(&path).is_err());
    fs::write(&path, b"not a png").unwrap();
    assert!(preview(&path).is_err());
    fs::File::create(&path)
        .unwrap()
        .set_len((PREVIEW_LIMIT + 1) as u64)
        .unwrap();
    assert!(preview(&path).is_err());
}

#[test]
fn catalog_filters_paid_invalid_geometry_traversal_and_duplicates() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("Minecraft.Windows.exe");
    fs::write(&game, []).unwrap();
    let root = temp.path().join("data/skin_packs/vanilla");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("steve.png"), png_bytes()).unwrap();
    fs::write(root.join("alex.png"), png_bytes()).unwrap();
    let entry = |name: &str, texture: &str, geometry: &str, kind: &str| json!({"localization_name":name,"texture":texture,"geometry":geometry,"type":kind});
    let classic = "geometry.humanoid.custom";
    fs::write(root.join("skins.json"), serde_json::to_vec(&json!({"skins":[
        entry("Steve", "steve.png", classic, "free"), entry("Alex", "alex.png", "geometry.humanoid.customSlim", "free"),
        entry("Duplicate", "steve.png", classic, "free"), entry("Paid", "steve.png", classic, "paid"),
        entry("Escape", "../steve.png", classic, "free"), entry("Unknown", "alex.png", "bad", "free")
    ]})).unwrap()).unwrap();
    let result = catalog(&game).unwrap();
    assert_eq!(result.skins.len(), 2);
    assert!(!result.skins[0].slim);
    assert!(result.skins[1].slim);
    assert!(result.warnings.is_empty());
}

#[test]
fn catalog_failures_do_not_hide_available_4d_skins() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("a/b/c/Minecraft.Windows.exe");
    fs::create_dir_all(game.parent().unwrap()).unwrap();
    fs::write(&game, []).unwrap();
    let (dir, archive, _) = NETEASE[0];
    let target = temp
        .path()
        .join("componentcache/support")
        .join(dir)
        .join(archive);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    write_zip(&target, &format!("{dir}/{dir}.png"), &png_bytes());
    let result = catalog(&game).unwrap();
    assert_eq!(result.skins.len(), 1);
    assert_eq!(result.skins[0].kind, "netease4d");
    assert_eq!(result.warnings.len(), 1);
}

fn write_zip(path: &Path, name: &str, bytes: &[u8]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
}

#[test]
fn zip_preview_accepts_only_known_descriptor_and_exact_entry() {
    let temp = tempfile::tempdir().unwrap();
    let (dir, archive, _) = NETEASE[0];
    let path = temp.path().join(dir).join(archive);
    fs::create_dir(path.parent().unwrap()).unwrap();
    write_zip(&path, &format!("{dir}/{dir}.png"), &png_bytes());
    assert!(preview(&path).is_ok());
    write_zip(&path, "../../outside.png", &png_bytes());
    assert!(preview(&path).is_err());
    assert!(!temp.path().join("outside.png").exists());
    let unknown = path.with_file_name("unknown.zip");
    write_zip(&unknown, &format!("{dir}/{dir}.png"), &png_bytes());
    assert!(preview(&unknown).is_err());
}

#[test]
fn zip_preview_rejects_oversized_uncompressed_entry() {
    let temp = tempfile::tempdir().unwrap();
    let (dir, archive, _) = NETEASE[0];
    let path = temp.path().join(dir).join(archive);
    fs::create_dir(path.parent().unwrap()).unwrap();
    write_zip(
        &path,
        &format!("{dir}/{dir}.png"),
        &vec![0; PREVIEW_LIMIT + 1],
    );
    assert!(preview(&path).is_err());
}
