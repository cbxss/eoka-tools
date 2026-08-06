use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationExposure {
    DefaultAgent,
    OptIn,
    Lifecycle,
}

impl OperationExposure {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DefaultAgent => "defaultAgent",
            Self::OptIn => "optIn",
            Self::Lifecycle => "lifecycle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationCapability {
    Navigation,
    Observation,
    Interaction,
    JavaScript,
    BrowserState,
    Tabs,
    Spa,
    Wasm,
    Network,
    Policy,
    Media,
    Captcha,
    Lifecycle,
}

impl OperationCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Navigation => "navigation",
            Self::Observation => "observation",
            Self::Interaction => "interaction",
            Self::JavaScript => "javascript",
            Self::BrowserState => "browser-state",
            Self::Tabs => "tabs",
            Self::Spa => "spa",
            Self::Wasm => "wasm",
            Self::Network => "network",
            Self::Policy => "policy",
            Self::Media => "media",
            Self::Captcha => "captcha",
            Self::Lifecycle => "lifecycle",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolManifestEntry {
    pub path: String,
    pub cmd: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub capability: &'static str,
    pub exposure: &'static str,
    pub read_only: bool,
    pub destructive: bool,
    pub input_schema: serde_json::Value,
    pub tags: Vec<&'static str>,
}
