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
    #[serde(default, skip_serializing)]
    description: String,
    #[serde(default, skip_serializing)]
    enabled: bool,
    #[serde(default, skip_serializing)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_category_is_enabled_and_default_on() {
        let category = Category::new("GRAMMAR", "Grammar");
        assert_eq!(category.id().as_str(), "GRAMMAR");
        assert_eq!(category.name(), "Grammar");
        assert_eq!(category.description(), "");
        assert!(category.is_enabled());
        assert!(category.is_default_on());
    }

    #[test]
    fn builders_override_flags_and_description() {
        let category = Category::new("STYLE", "Style")
            .with_description("Style issues")
            .with_enabled(false)
            .with_default_on(false);
        assert_eq!(category.description(), "Style issues");
        assert!(!category.is_enabled());
        assert!(!category.is_default_on());
    }

    #[test]
    fn default_category_is_misc() {
        let category = Category::default();
        assert_eq!(category.id().as_str(), "MISC");
        assert_eq!(category.name(), "Miscellaneous");
    }

    #[test]
    fn categories_compare_by_id_value() {
        assert_eq!(CategoryId::new("A"), CategoryId::new("A"));
        assert_ne!(CategoryId::new("A"), CategoryId::new("B"));
        let mut set = std::collections::HashSet::new();
        set.insert(CategoryId::new("A"));
        assert!(set.contains(&CategoryId::new("A")));
        assert_eq!(CategoryId::new("GRAMMAR").as_str(), "GRAMMAR");
    }

    #[test]
    fn serialization_keeps_only_id_and_name() {
        let category = Category::new("GRAMMAR", "Grammar")
            .with_description("hidden")
            .with_enabled(false)
            .with_default_on(false);
        let json = serde_json::to_value(&category).unwrap();
        assert_eq!(json["id"], "GRAMMAR");
        assert_eq!(json["name"], "Grammar");
        assert!(json.get("description").is_none());
        assert!(json.get("enabled").is_none());
        assert!(json.get("default_on").is_none());
        let back: Category = serde_json::from_value(json).unwrap();
        assert_eq!(back.id().as_str(), "GRAMMAR");
        assert_eq!(back.name(), "Grammar");
    }
}
