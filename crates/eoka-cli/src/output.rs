//! Output formatting: plain text or JSON.

use crate::protocol::Response;

pub fn print_response(response: &Response, json_mode: bool) {
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(response).unwrap_or_default()
        );
        return;
    }

    if !response.ok {
        if let Some(ref err) = response.error {
            eprintln!("Error: {}", err);
        }
        return;
    }

    if let Some(ref data) = response.data {
        match data {
            serde_json::Value::String(s) => println!("{}", s),
            other => println!(
                "{}",
                serde_json::to_string_pretty(other).unwrap_or_default()
            ),
        }
    }
}
