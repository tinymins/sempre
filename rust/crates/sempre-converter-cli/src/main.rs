use std::io::{self, Read};

use sempre_converter::{CompileRequest, compile};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorResponse {
    protocol: u32,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

fn main() {
    if let Err(error) = run() {
        let response = ErrorResponse {
            protocol: 1,
            error: ErrorBody {
                code: "COMPILE_FAILED",
                message: error,
            },
        };
        let output =
            serde_json::to_string(&response).expect("error response serialization cannot fail");
        println!("{output}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let request: CompileRequest = serde_json::from_str(&input)
        .map_err(|error| format!("invalid compile request: {error}"))?;
    if request.protocol != 1 {
        return Err(format!(
            "unsupported compiler protocol {}",
            request.protocol
        ));
    }
    let result = compile(&request).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|error| error.to_string())?
    );
    Ok(())
}
