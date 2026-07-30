use std::collections::HashMap;
use std::fmt::Write;

use anyhow::Result;
use eoka_server::eoka::Page;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetFullAXTreeParams {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AXTreeResult {
    nodes: Vec<AXNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AXNode {
    node_id: String,
    #[serde(default)]
    ignored: bool,
    role: Option<AXValue>,
    name: Option<AXValue>,
    value: Option<AXValue>,
    #[serde(default)]
    properties: Vec<AXProperty>,
    parent_id: Option<String>,
    #[serde(alias = "backendDOMNodeId")]
    backend_dom_node_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AXValue {
    value: Option<serde_json::Value>,
}

impl AXValue {
    fn as_str(&self) -> &str {
        self.value.as_ref().and_then(|v| v.as_str()).unwrap_or("")
    }

    fn is_truthy(&self) -> bool {
        self.value
            .as_ref()
            .map(|v| v.as_bool().unwrap_or(false) || v.as_str() == Some("true"))
            .unwrap_or(false)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AXProperty {
    name: String,
    value: AXValue,
}

#[derive(Debug, Clone)]
pub struct SnapshotRef {
    pub ref_label: String,
    pub backend_node_id: i64,
    pub role: String,
    pub name: String,
}

#[derive(Debug)]
pub struct SnapshotResult {
    pub tree_text: String,
    pub refs: Vec<SnapshotRef>,
}

const NOISE_ROLES: &[&str] = &[
    "generic",
    "none",
    "presentation",
    "InlineTextBox",
    "LineBreak",
];

fn should_skip(role: &str, name: &str, ignored: bool, include_all: bool) -> bool {
    if ignored {
        return true;
    }
    if include_all {
        return false;
    }
    if NOISE_ROLES.contains(&role) {
        return true;
    }
    role == "StaticText" && name.is_empty()
}

fn opt_str(v: &Option<AXValue>) -> &str {
    v.as_ref().map(|av| av.as_str()).unwrap_or("")
}

fn format_props(node: &AXNode, out: &mut String) {
    let val = opt_str(&node.value);
    if !val.is_empty() {
        let _ = write!(out, " value=\"{}\"", val);
    }

    for prop in &node.properties {
        let token: Option<&str> = match prop.name.as_str() {
            "disabled" if prop.value.is_truthy() => Some("disabled"),
            "required" if prop.value.is_truthy() => Some("required"),
            "selected" if prop.value.is_truthy() => Some("selected"),
            "focused" if prop.value.is_truthy() => Some("focused"),
            "checked" => match prop.value.as_str() {
                "true" => Some("checked"),
                "mixed" => Some("checked=mixed"),
                _ => None,
            },
            "expanded" => Some(if prop.value.is_truthy() {
                "expanded"
            } else {
                "collapsed"
            }),
            "invalid" => match prop.value.as_str() {
                "true" | "spelling" | "grammar" => Some("invalid"),
                _ => None,
            },
            _ => None,
        };
        if let Some(t) = token {
            let _ = write!(out, " {}", t);
        }
    }
}

struct Walker<'a> {
    nodes_by_id: HashMap<&'a str, &'a AXNode>,
    children_of: HashMap<&'a str, Vec<&'a str>>,
    include_all: bool,
    ref_counter: usize,
    refs: Vec<SnapshotRef>,
    output: String,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, node_id: &str, depth: usize) {
        let node = match self.nodes_by_id.get(node_id) {
            Some(n) => *n,
            None => return,
        };

        let role = opt_str(&node.role);
        let name = opt_str(&node.name);
        let skip = should_skip(role, name, node.ignored, self.include_all);

        if !skip {
            self.emit_node(node, role, name, depth);
        }

        let child_depth = if skip { depth } else { depth + 1 };
        if let Some(children) = self.children_of.get(node_id) {
            for child_id in children.clone() {
                self.walk(child_id, child_depth);
            }
        }
    }

    fn emit_node(&mut self, node: &AXNode, role: &str, name: &str, depth: usize) {
        let ref_label = match node.backend_dom_node_id {
            Some(id) if id > 0 => {
                self.ref_counter += 1;
                let label = format!("@e{}", self.ref_counter);
                self.refs.push(SnapshotRef {
                    ref_label: label.clone(),
                    backend_node_id: id,
                    role: role.to_string(),
                    name: name.to_string(),
                });
                Some(label)
            }
            _ => None,
        };

        for _ in 0..depth {
            self.output.push_str("  ");
        }
        if let Some(ref label) = ref_label {
            let _ = write!(self.output, "{} ", label);
        }
        self.output.push_str(role);
        if !name.is_empty() {
            let _ = write!(self.output, " \"{}\"", name);
        }
        format_props(node, &mut self.output);
        self.output.push('\n');
    }
}

pub async fn snapshot(page: &Page, include_all: bool) -> Result<SnapshotResult> {
    let _ = page
        .session()
        .send::<_, serde_json::Value>("DOM.enable", &serde_json::json!({}))
        .await;
    let _ = page
        .session()
        .send::<_, serde_json::Value>(
            "DOM.getDocument",
            &serde_json::json!({"depth": -1, "pierce": true}),
        )
        .await;

    let result: AXTreeResult = page
        .session()
        .send("Accessibility.getFullAXTree", &GetFullAXTreeParams {})
        .await
        .map_err(|e| anyhow::anyhow!("CDP Accessibility.getFullAXTree failed: {}", e))?;

    if result.nodes.is_empty() {
        return Ok(SnapshotResult {
            tree_text: "No accessibility tree available.".into(),
            refs: Vec::new(),
        });
    }

    let (nodes_by_id, children_of, roots) = build_tree_index(&result.nodes);

    let mut walker = Walker {
        nodes_by_id,
        children_of,
        include_all,
        ref_counter: 0,
        refs: Vec::new(),
        output: String::with_capacity(4096),
    };

    for root in roots {
        walker.walk(root, 0);
    }

    Ok(SnapshotResult {
        tree_text: walker.output,
        refs: walker.refs,
    })
}

type TreeIndex<'a> = (
    HashMap<&'a str, &'a AXNode>,
    HashMap<&'a str, Vec<&'a str>>,
    Vec<&'a str>,
);

fn build_tree_index(nodes: &[AXNode]) -> TreeIndex<'_> {
    let mut by_id: HashMap<&str, &AXNode> = HashMap::with_capacity(nodes.len());
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut roots: Vec<&str> = Vec::new();

    for node in nodes {
        by_id.insert(&node.node_id, node);
    }

    for node in nodes {
        match node.parent_id.as_deref() {
            Some(pid) if by_id.contains_key(pid) => {
                children.entry(pid).or_default().push(&node.node_id);
            }
            _ => roots.push(&node.node_id),
        }
    }

    (by_id, children, roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_ignored_nodes() {
        assert!(should_skip("button", "Click", true, false));
        assert!(should_skip("button", "Click", true, true));
    }

    #[test]
    fn skip_noise_roles() {
        for role in NOISE_ROLES {
            assert!(should_skip(role, "", false, false), "should skip {}", role);
        }
    }

    #[test]
    fn skip_unnamed_static_text() {
        assert!(should_skip("StaticText", "", false, false));
        assert!(!should_skip("StaticText", "Hello", false, false));
    }

    #[test]
    fn keep_normal_roles() {
        for (role, name) in [
            ("button", "Submit"),
            ("link", "Home"),
            ("textbox", "Email"),
            ("heading", "Title"),
        ] {
            assert!(!should_skip(role, name, false, false));
        }
    }

    #[test]
    fn include_all_keeps_noise() {
        assert!(!should_skip("generic", "", false, true));
        assert!(!should_skip("none", "", false, true));
    }

    #[test]
    fn ax_value_helpers() {
        let truthy = AXValue {
            value: Some(serde_json::json!(true)),
        };
        assert!(truthy.is_truthy());
        assert_eq!(truthy.as_str(), "");

        let str_true = AXValue {
            value: Some(serde_json::json!("true")),
        };
        assert!(str_true.is_truthy());
        assert_eq!(str_true.as_str(), "true");

        let falsy = AXValue {
            value: Some(serde_json::json!(false)),
        };
        assert!(!falsy.is_truthy());

        let none = AXValue { value: None };
        assert!(!none.is_truthy());
        assert_eq!(none.as_str(), "");
    }

    #[test]
    fn format_props_value_and_disabled() {
        let node = AXNode {
            node_id: "1".into(),
            ignored: false,
            role: None,
            name: None,
            value: Some(AXValue {
                value: Some(serde_json::json!("hello")),
            }),
            properties: vec![AXProperty {
                name: "disabled".into(),
                value: AXValue {
                    value: Some(serde_json::json!(true)),
                },
            }],
            parent_id: None,
            backend_dom_node_id: None,
        };
        let mut out = String::new();
        format_props(&node, &mut out);
        assert!(out.contains("value=\"hello\""));
        assert!(out.contains("disabled"));
    }
}
