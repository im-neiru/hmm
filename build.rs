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

        res.set_version_info(
            winresource::VersionInfo::PRODUCTVERSION,
            0u64 << 48 | 1u64 << 32 | 0u64 << 16 | 0u64,
        );
        res.set("CompanyName", "im-neiru");
        res.set("LegalCopyright", "Copyright © 2026  im-neiru");
        res.set("ProductName", "HMM TTS");
        res.set_language(0x0409);

        res.compile().unwrap();
    }
}

async fn generate() {
    join!(preprocess::process_models(), preprocess::process_voices());
}
