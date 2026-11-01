use std::{env, fs, path::PathBuf};
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let assets = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../assets");
    println!("cargo:rerun-if-changed={}", assets.display());
    let mut hands: Vec<_> = fs::read_dir(&assets)
        .expect("Missing upstream assets")
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("hand_")
                && p.extension().is_some_and(|e| e == "png")
        })
        .collect();
    hands.sort();
    assert_eq!(hands.len(), 36, "Expected all 36 upstream hands");
    let mut source = String::from("pub const HANDS: &[(&str, &[u8])] = &[\n");
    for p in hands {
        let name = p
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_start_matches("hand_");
        let compressed = out.join(format!("hand_{name}.png"));
        image::open(&p)
            .expect("Invalid hand image")
            .resize(1024, 1024, image::imageops::FilterType::Lanczos3)
            .save(&compressed)
            .expect("Could not prepare hand texture");
        source.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            compressed.to_str().unwrap()
        ));
    }
    source.push_str("];\npub const SOUNDS: &[(&str, &[u8])] = &[\n");
    for name in ["boop", "click", "pop", "squeak", "thock", "wood"] {
        let p = assets.join(format!("sounds/tap_{name}.wav"));
        source.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            p.to_str().unwrap()
        ));
    }
    source.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("assets.rs"),
        source,
    )
    .unwrap();
}
