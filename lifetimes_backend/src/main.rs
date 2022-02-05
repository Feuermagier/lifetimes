use lifetimes_backend::check;

fn main() {
    //tracing_subscriber::fmt().compact().init();
    env_logger::builder()
        .filter_level(log::LevelFilter::Error)
        .filter_module("lifetimes_backend", log::LevelFilter::Trace)
        .init();

    check(
        r#"
        fn main() {
            let mut x = 42;
            let y = &mut x;
            let z = &mut x;
        }
"#
        .to_string(),
    )
    .unwrap();
}
