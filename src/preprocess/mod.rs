use std::{
    env,
    path::{Path, PathBuf},
};

mod kokoro;

pub async fn process_models() {
    tokio::try_join!(kokoro::process_model()).unwrap();
}

pub async fn process_voices() {
    tokio::try_join!(kokoro::process_voices()).unwrap();
}

const MODEL_OUT_DIR: &str = "src/models/base";
const MODEL_DIR: &str = "models";

fn resolve_manifest_path<P: AsRef<Path>>(parent: P, relative_path: P) -> PathBuf {
    let base = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .expect("CARGO_MANIFEST_DIR environment variable is missing");

    base.join(parent).join(relative_path)
}
