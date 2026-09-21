use platform_server::{
    config::Config,
    database,
    password::PasswordService,
    users::{self, NewUser},
};
use std::process::ExitCode;

// Bootstrap tool for the very first user until the invitation flow exists in
// the organizations phase. Passwords arrive only via USER_PASSWORD so they
// never appear in shell history, arguments, logs, or error output.
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
    if args.len() != 3 || args[0] != "create" {
        return Err("Usage: user-admin create <email> <display_name> (password via USER_PASSWORD)");
    }
    let email = args[1].trim();
    let display_name = args[2].trim();
    if email.is_empty() || !email.contains('@') || email.len() > 254 {
        return Err("EMAIL_INVALID");
    }
    if display_name.is_empty() || display_name.len() > 200 {
        return Err("DISPLAY_NAME_INVALID");
    }
    let password = std::env::var("USER_PASSWORD").map_err(|_| "USER_PASSWORD is required")?;
    if password.is_empty() || password.len() > 1024 {
        return Err("USER_PASSWORD_INVALID");
    }
    let config = Config::from_env()?;
    let passwords =
        PasswordService::new(&config.auth).map_err(|_| "PASSWORD_SERVICE_INIT_FAILED")?;
    let pool = database::pool(&config);
    let result = users::insert_user(
        &pool,
        &NewUser {
            email: email.to_owned(),
            password_hash: passwords
                .hash(&password)
                .map_err(|_| "PASSWORD_HASH_FAILED")?,
            display_name: display_name.to_owned(),
            locale: None,
        },
    )
    .await;
    pool.close().await;
    match result {
        Ok(user) => {
            println!("User created: {}", user.id);
            Ok(())
        }
        Err(users::StoreError::EmailAlreadyExists) => Err("EMAIL_ALREADY_EXISTS"),
        Err(users::StoreError::DatabaseError) => Err("USER_CREATE_FAILED"),
    }
}
