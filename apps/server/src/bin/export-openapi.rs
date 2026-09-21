use std::process::ExitCode;

fn main() -> ExitCode {
    match platform_server::openapi().to_pretty_json() {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(_) => {
            eprintln!("OPENAPI_EXPORT_FAILED");
            ExitCode::FAILURE
        }
    }
}
