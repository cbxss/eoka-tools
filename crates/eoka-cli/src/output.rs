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
            serde_json::Value::Object(obj) if obj.get("text").is_some() => {
                if let Some(text) = obj.get("text").and_then(|value| value.as_str()) {
                    println!("{}", text);
                }
            }
            other => println!(
                "{}",
                serde_json::to_string_pretty(other).unwrap_or_default()
            ),
        }
    }
}
