use async_trait::async_trait;
use eoka_protocol::{
    all_operations, input_schema_for_operation, request_from_cmd, tags_for_operation,
    OperationCapability, OperationExposure, OperationSpec,
};
use eoka_sdk::EokaClient;
use std::collections::BTreeSet;
use std::sync::Arc;
use tack_core::{
    CatalogError, Tool, ToolAnnotations, ToolCall, ToolCallError, ToolCallOutput, ToolCatalog,
    ToolDescriptor, ToolSet, ToolSetError,
};

#[derive(Clone, Debug)]
pub struct EokaToolFilter {
    include_exposures: BTreeSet<ExposureKey>,
    include_capabilities: BTreeSet<CapabilityKey>,
}

impl EokaToolFilter {
    pub fn default_agent() -> Self {
        let mut include_exposures = BTreeSet::new();
        include_exposures.insert(ExposureKey::DefaultAgent);
        Self {
            include_exposures,
            include_capabilities: BTreeSet::new(),
        }
    }

    pub fn all_non_lifecycle() -> Self {
        let mut include_exposures = BTreeSet::new();
        include_exposures.insert(ExposureKey::DefaultAgent);
        include_exposures.insert(ExposureKey::OptIn);
        Self {
            include_exposures,
            include_capabilities: BTreeSet::new(),
        }
    }

    pub fn include_capability(mut self, capability: OperationCapability) -> Self {
        self.include_capabilities
            .insert(CapabilityKey::from(capability));
        self
    }

    fn includes(&self, operation: &OperationSpec) -> bool {
        if self
            .include_capabilities
            .contains(&CapabilityKey::from(operation.capability))
        {
            return operation.exposure != OperationExposure::Lifecycle;
        }
        self.include_exposures
            .contains(&ExposureKey::from(operation.exposure))
    }
}

impl Default for EokaToolFilter {
    fn default() -> Self {
        Self::default_agent()
    }
}

#[derive(Clone)]
pub struct EokaToolSet {
    client: Arc<EokaClient>,
    filter: EokaToolFilter,
}

impl EokaToolSet {
    pub fn new(client: EokaClient) -> Self {
        Self {
            client: Arc::new(client),
            filter: EokaToolFilter::default_agent(),
        }
    }

    pub fn with_filter(mut self, filter: EokaToolFilter) -> Self {
        self.filter = filter;
        self
    }

    pub fn catalog(&self, namespace: &str) -> Result<ToolCatalog, CatalogError> {
        let mut catalog = ToolCatalog::new();
        catalog.mount(namespace, self)?;
        Ok(catalog)
    }

    pub fn operations(&self) -> impl Iterator<Item = &'static OperationSpec> + '_ {
        all_operations()
            .iter()
            .filter(|operation| self.filter.includes(operation))
    }
}

impl ToolSet for EokaToolSet {
    fn tools(&self) -> Result<Vec<Arc<dyn Tool>>, ToolSetError> {
        Ok(self
            .operations()
            .map(|operation| {
                Arc::new(EokaTool::new(Arc::clone(&self.client), operation)) as Arc<dyn Tool>
            })
            .collect())
    }
}

struct EokaTool {
    client: Arc<EokaClient>,
    operation: &'static OperationSpec,
    descriptor: ToolDescriptor,
}

impl EokaTool {
    fn new(client: Arc<EokaClient>, operation: &'static OperationSpec) -> Self {
        let descriptor = ToolDescriptor::new(operation.path, operation.name)
            .with_description(operation.description)
            .with_input_schema(input_schema_for_operation(operation))
            .with_tags(tags_for_operation(operation))
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(operation.read_only)
                    .destructive(operation.destructive)
                    .capability(operation.capability.as_str()),
            );
        Self {
            client,
            operation,
            descriptor,
        }
    }
}

#[async_trait]
impl Tool for EokaTool {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    async fn call(&self, call: ToolCall) -> ToolCallOutput {
        let request = match request_from_cmd(self.operation.cmd, call.input) {
            Ok(request) => request,
            Err(error) => return failed_tool("invalid_input", error),
        };
        match self.client.call(request).await {
            Ok(response) if response.ok => ToolCallOutput {
                ok: true,
                data: Some(response.data.unwrap_or(serde_json::Value::Null)),
                text: None,
                raw: None,
                error: None,
            },
            Ok(response) => failed_tool(
                "eoka_error",
                response
                    .error
                    .unwrap_or_else(|| "Eoka command failed".to_string()),
            ),
            Err(error) => failed_tool("transport_error", error.to_string()),
        }
    }
}

fn failed_tool(code: &'static str, message: String) -> ToolCallOutput {
    ToolCallOutput {
        ok: false,
        data: None,
        text: None,
        raw: None,
        error: Some(ToolCallError {
            message,
            code: Some(code.to_string()),
            details: None,
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ExposureKey {
    DefaultAgent,
    OptIn,
    Lifecycle,
}

impl From<OperationExposure> for ExposureKey {
    fn from(value: OperationExposure) -> Self {
        match value {
            OperationExposure::DefaultAgent => Self::DefaultAgent,
            OperationExposure::OptIn => Self::OptIn,
            OperationExposure::Lifecycle => Self::Lifecycle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CapabilityKey {
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

impl From<OperationCapability> for CapabilityKey {
    fn from(value: OperationCapability) -> Self {
        match value {
            OperationCapability::Navigation => Self::Navigation,
            OperationCapability::Observation => Self::Observation,
            OperationCapability::Interaction => Self::Interaction,
            OperationCapability::JavaScript => Self::JavaScript,
            OperationCapability::BrowserState => Self::BrowserState,
            OperationCapability::Tabs => Self::Tabs,
            OperationCapability::Spa => Self::Spa,
            OperationCapability::Wasm => Self::Wasm,
            OperationCapability::Network => Self::Network,
            OperationCapability::Policy => Self::Policy,
            OperationCapability::Media => Self::Media,
            OperationCapability::Captcha => Self::Captcha,
            OperationCapability::Lifecycle => Self::Lifecycle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eoka_protocol::manifest_for_operations;
    use eoka_sdk::LaunchSpec;

    fn test_set() -> EokaToolSet {
        EokaToolSet::new(EokaClient::new(
            "test",
            LaunchSpec::Launch {
                headless: true,
                from_profile: None,
                clone_state_from: None,
                no_stealth: false,
                proxy: None,
                no_js: false,
                js_allow: Vec::new(),
                js_block: Vec::new(),
                persist: false,
                geo_align: false,
            },
        ))
    }

    #[test]
    fn default_catalog_uses_stable_nested_paths() {
        let catalog = test_set().catalog("eoka").unwrap();
        assert!(catalog.contains_path("eoka.tab.list"));
        assert!(catalog.contains_path("eoka.spa.navigate"));
        assert!(catalog.contains_path("eoka.double_click"));
        assert!(!catalog.contains_path("eoka.network.log"));
        assert!(!catalog.contains_path("eoka.close"));
    }

    #[test]
    fn opt_in_filter_adds_network_without_lifecycle() {
        let set = test_set().with_filter(
            EokaToolFilter::default_agent().include_capability(OperationCapability::Network),
        );
        let catalog = set.catalog("eoka").unwrap();
        assert!(catalog.contains_path("eoka.network.log"));
        assert!(!catalog.contains_path("eoka.close"));
    }

    #[test]
    fn default_descriptors_match_protocol_manifest_projection() {
        let catalog = test_set().catalog("eoka").unwrap();
        let manifest = manifest_for_operations("eoka", false);

        for entry in manifest {
            let descriptor = catalog.by_path(&entry.path).unwrap().descriptor();
            assert_eq!(descriptor.path(), entry.path);
            assert_eq!(descriptor.name(), entry.name);
            assert_eq!(descriptor.description(), Some(entry.description));
            assert_eq!(descriptor.input_schema(), &entry.input_schema);
            assert_eq!(
                descriptor.tags(),
                &entry
                    .tags
                    .iter()
                    .map(|tag| tag.to_string())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                descriptor.annotations().capability.as_deref(),
                Some(entry.capability)
            );
            assert_eq!(descriptor.annotations().read_only, Some(entry.read_only));
            assert_eq!(
                descriptor.annotations().destructive,
                Some(entry.destructive)
            );
        }
    }
}
