//! The core with this crate's apps, for development and the browser tests
//! (`cargo run --example dev`). The released programs are built in the
//! `ortus` repository.

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let status = ortus_core::cli::run(
        "ortus-tasks-dev",
        ortus_core::VERSION,
        ortus_tasks::apps(),
        &args,
    )
    .await;
    std::process::exit(status);
}
