#[path = "src/preprocess/mod.rs"]
mod preprocess;

use tokio::{join, runtime::Builder};

fn main() {
    let rt = Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .unwrap();

    rt.block_on(async { generate().await });

    #[cfg(target_os = "windows")] // add icon for windows app
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/images/icon.ico");
        res.compile().unwrap();
    }
}

async fn generate() {
    join!(preprocess::process_models(), preprocess::process_voices());
}
