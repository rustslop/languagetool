use og_api::server::run_server;

fn main() {
    tracing_subscriber::fmt::init();

    // Checks and engine builds run on the blocking pool (see handle_check).
    // Tokio's default blocking pool allows 512 threads, which oversubscribes
    // the cores for tens-of-millisecond checks and turns request bursts into
    // thread sprawl. Cap the pool near the CPU count with a little headroom
    // so a cold engine build does not starve running checks; work beyond the
    // cap queues in the pool, giving bursts backpressure instead.
    let blocking_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        + 2;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(blocking_threads)
        .build()
        .unwrap_or_else(|err| {
            eprintln!("Failed to build tokio runtime: {err}");
            std::process::exit(1);
        });

    if let Err(e) = runtime.block_on(run_server(8081)) {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}
