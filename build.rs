#[path = "src/preprocess/kokoro/mod.rs"]
mod kokoro;

use burn_onnx::ModelGen;
use std::env;
use std::path::{Path, PathBuf};

struct ModelConfig {
    input_rel_path: &'static str,
    out_dir_rel_path: &'static str,
}

const MODELS_TO_GENERATE: &[ModelConfig] = &[ModelConfig {
    input_rel_path: "kokoro_82m_v1.onnx",
    out_dir_rel_path: "kokoro_82m_v1",
}];

fn main() {
    for config in MODELS_TO_GENERATE {
        let input_path = resolve_manifest_path("models", config.input_rel_path);
        let out_dir = resolve_manifest_path("src/models/base", config.out_dir_rel_path);

        println!("cargo:rerun-if-changed={}", input_path.display());

        ModelGen::new()
            .input(&input_path.into_string().unwrap())
            .out_dir(&out_dir.into_string().unwrap())
            .run_from_script();
    }
}

fn resolve_manifest_path<P: AsRef<Path>>(parent: P, relative_path: P) -> PathBuf {
    let base = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .expect("CARGO_MANIFEST_DIR environment variable is missing");

    base.join(parent).join(relative_path)
}
