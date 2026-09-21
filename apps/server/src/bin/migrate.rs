use platform_server::{
    config::Config,
    database,
    migrations::{self, MIGRATOR},
};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("{code}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 || !["up", "verify", "revert"].contains(&args[0].as_str()) {
        return Err("Usage: migrate <up|verify|revert>");
    }
    let config = Config::from_env()?;
    let pool = database::pool(&config);
    let result = match args[0].as_str() {
        "up" => MIGRATOR
            .run(&pool)
            .await
            .map_err(|_| "MIGRATION_APPLY_FAILED"),
        "verify" => migrations::verify(&pool).await,
        "revert" => migrations::revert_last(&pool).await,
        _ => Err("UNKNOWN_MIGRATION_COMMAND"),
    };
    pool.close().await;
    result?;
    println!("Migration command completed: {}", args[0]);
    Ok(())
}
