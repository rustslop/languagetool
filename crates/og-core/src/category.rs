use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CategoryId(String);

impl CategoryId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    id: CategoryId,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    default_on: bool,
}

impl Category {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: CategoryId::new(id),
            name: name.into(),
            description: String::new(),
            enabled: true,
            default_on: true,
        }
    }

    pub fn id(&self) -> &CategoryId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_default_on(&self) -> bool {
        self.default_on
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_default_on(mut self, default_on: bool) -> Self {
        self.default_on = default_on;
        self
    }
}

impl Default for Category {
    fn default() -> Self {
        Self::new("MISC", "Miscellaneous")
    }
}
