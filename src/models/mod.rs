use std::{env, io, path::PathBuf};

pub(crate) mod kokoro;

fn resolve_data_path(name: &str) -> Result<PathBuf, io::Error> {
    let exe_path = env::current_exe()?;

    let parent_dir = exe_path.parent().ok_or(io::Error::new(
        io::ErrorKind::Other,
        "Failed to get parent directory of executable",
    ))?;

    let data_dir = parent_dir.join("data");

    Ok(data_dir)
}
