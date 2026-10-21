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
}

async fn generate() {
    join!(preprocess::process_models());
}
