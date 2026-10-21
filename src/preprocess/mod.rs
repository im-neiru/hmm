use std::{
    env, io,
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

fn get_data_dir(out_dir: &Path) -> io::Result<PathBuf> {
    let build_dir = out_dir
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("build"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not locate Cargo build directory from OUT_DIR",
            )
        })?;

    Ok(build_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not locate Cargo profile directory",
            )
        })?
        .join("data"))
}

fn resolve_manifest_path<P: AsRef<Path>>(parent: P, relative_path: P) -> PathBuf {
    let base = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .expect("CARGO_MANIFEST_DIR environment variable is missing");

    base.join(parent).join(relative_path)
}
