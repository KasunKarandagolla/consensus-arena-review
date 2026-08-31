use std::collections::HashMap;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Capability {
    TextChat,
    ImageUnderstanding,
    ImageGeneration,
    CodeExecution,
    WebSearch,
    DocumentAnalysis,
    YouTubeAnalysis,
    ArtifactGeneration,
    LongContext,
    Reasoning,
}

#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    models: HashMap<String, Vec<Capability>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        CapabilityRegistry {
            models: HashMap::new(),
        }
    }

    pub fn register(&mut self, model_name: String, capabilities: Vec<Capability>) {
        self.models.insert(model_name, capabilities);
    }

    pub fn get_capable_models(&self, required: &[Capability]) -> Vec<String> {
        self.models
            .iter()
            .filter(|(_, caps)| required.iter().all(|r| caps.contains(r)))
            .map(|(name, _)| name.clone())
            .collect()
    }
}
