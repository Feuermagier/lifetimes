use lifetimes_backend::check;

fn main() {
    //tracing_subscriber::fmt().compact().init();
    env_logger::builder()
        .filter_level(log::LevelFilter::Error)
        .filter_module("lifetimes_backend", log::LevelFilter::Trace)
        .init();

    check(std::fs::read_to_string("../scratch/src/main.rs").unwrap()).unwrap();
}
