use crate::eoka::{Page, Result};
use serde::Deserialize;

use crate::InteractiveElement;

#[derive(Deserialize)]
struct RawElement {
    tag: String,
    role: Option<String>,
    text: String,
    placeholder: Option<String>,
    input_type: Option<String>,
    selector: String,
    checked: bool,
    value: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

const OBSERVE_JS: &str = include_str!("js/observe.js");

pub fn parse_raw_elements(json_str: &str) -> std::result::Result<Vec<InteractiveElement>, String> {
    let raw: Vec<RawElement> =
        serde_json::from_str(json_str).map_err(|e| format!("observe parse error: {}", e))?;

    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            let fingerprint = InteractiveElement::compute_fingerprint(
                &r.tag,
                &r.text,
                r.role.as_deref(),
                r.input_type.as_deref(),
                r.placeholder.as_deref(),
                &r.selector,
            );
            InteractiveElement {
                index: i,
                tag: r.tag,
                role: r.role,
                text: r.text,
                placeholder: r.placeholder,
                input_type: r.input_type,
                selector: r.selector,
                checked: r.checked,
                value: if r.value.is_empty() {
                    None
                } else {
                    Some(r.value)
                },
                bbox: crate::eoka::BoundingBox {
                    x: r.x,
                    y: r.y,
                    width: r.width,
                    height: r.height,
                },
                fingerprint,
            }
        })
        .collect())
}

pub async fn observe(page: &Page, viewport_only: bool) -> Result<Vec<InteractiveElement>> {
    let js = format!(
        "var __eoka_viewport_only = {}; {}",
        viewport_only, OBSERVE_JS
    );
    let json_str: String = page.evaluate(&js).await?;

    parse_raw_elements(&json_str).map_err(crate::eoka::Error::cdp_msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_button() {
        let json = r#"[{
            "tag": "button",
            "role": null,
            "text": "Submit",
            "placeholder": null,
            "input_type": null,
            "selector": "button.primary",
            "checked": false,
            "value": "",
            "x": 100,
            "y": 200,
            "width": 80,
            "height": 30
        }]"#;
        let elements = parse_raw_elements(json).unwrap();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].index, 0);
        assert_eq!(elements[0].tag, "button");
        assert_eq!(elements[0].text, "Submit");
        assert_eq!(elements[0].selector, "button.primary");
        assert!(elements[0].value.is_none());
    }

    #[test]
    fn parse_input_with_value() {
        let json = r#"[{
            "tag": "input",
            "role": null,
            "text": "Email",
            "placeholder": "Enter email",
            "input_type": "email",
            "selector": "input[name=\"email\"]",
            "checked": false,
            "value": "user@test.com",
            "x": 10,
            "y": 20,
            "width": 200,
            "height": 40
        }]"#;
        let elements = parse_raw_elements(json).unwrap();
        assert_eq!(elements[0].input_type.as_deref(), Some("email"));
        assert_eq!(elements[0].placeholder.as_deref(), Some("Enter email"));
        assert_eq!(elements[0].value.as_deref(), Some("user@test.com"));
    }

    #[test]
    fn sequential_index_assignment() {
        let json = r#"[
            {"tag":"a","role":null,"text":"Link 1","placeholder":null,"input_type":null,"selector":"a:nth-of-type(1)","checked":false,"value":"","x":0,"y":0,"width":50,"height":20},
            {"tag":"a","role":null,"text":"Link 2","placeholder":null,"input_type":null,"selector":"a:nth-of-type(2)","checked":false,"value":"","x":0,"y":30,"width":50,"height":20},
            {"tag":"button","role":null,"text":"Click","placeholder":null,"input_type":null,"selector":"button","checked":false,"value":"","x":0,"y":60,"width":50,"height":20}
        ]"#;
        let elements = parse_raw_elements(json).unwrap();
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].index, 0);
        assert_eq!(elements[1].index, 1);
        assert_eq!(elements[2].index, 2);
    }

    #[test]
    fn empty_array() {
        let elements = parse_raw_elements("[]").unwrap();
        assert!(elements.is_empty());
    }

    #[test]
    fn invalid_json() {
        assert!(parse_raw_elements("not json").is_err());
        assert!(parse_raw_elements("{\"bad\": true}").is_err());
    }

    #[test]
    fn fingerprint_consistency() {
        let json = r#"[{
            "tag": "button",
            "role": null,
            "text": "Submit",
            "placeholder": null,
            "input_type": null,
            "selector": "button.primary",
            "checked": false,
            "value": "",
            "x": 100,
            "y": 200,
            "width": 80,
            "height": 30
        }]"#;
        let e1 = parse_raw_elements(json).unwrap();
        let e2 = parse_raw_elements(json).unwrap();
        assert_eq!(e1[0].fingerprint, e2[0].fingerprint);
        assert_ne!(e1[0].fingerprint, 0);
    }
}
