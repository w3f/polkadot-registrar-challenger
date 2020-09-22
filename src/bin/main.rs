use failure::Error;
use registrar::{block, run, Config};
use std::env;
use std::fs::File;
use std::io::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Open config file.
    let mut file = File::open("config.json")
        .or_else(|_| File::open("/etc/registrar/config.json"))
        .map_err(|_| {
            eprintln!("Failed to open config at 'config.json' or '/etc/registrar/config.json'.");
            std::process::exit(1);
        })
        .unwrap();

    // Parse config file as JSON.
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let config = serde_json::from_str::<Config>(&contents)
        .map_err(|err| {
            eprintln!("Failed to parse config: {}", err);
            std::process::exit(1);
        })
        .unwrap();

    // Env variables for log level overwrites config.
    if let Ok(_) = env::var("RUST_LOG") {
        println!("Env variable 'RUST_LOG' found, overwriting logging level from config.");
        env_logger::init();
    } else {
        println!("Setting log level to '{}' from config.", config.log_level);
        env_logger::builder()
            .filter_module("registrar", config.log_level)
            .init();
    }

    println!("Logger initiated");

    run(config).await?;
    block().await;

    Ok(())
}
