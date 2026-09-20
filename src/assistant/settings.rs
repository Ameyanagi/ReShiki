//! Catalog-driven assistant choices. Automatic selection never pins an old model.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Effort {
    #[serde(rename = "reasoningEffort")]
    pub id: String,
    pub description: String,
}
impl Effort {
    pub fn label(&self) -> &str {
        effort_label(&self.id)
    }
}
pub fn effort_label(id: &str) -> &str {
    match id {
        "none" => "None",
        "minimal" => "Minimal",
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra high",
        "max" => "Max",
        "ultra" => "Ultra",
        other => other,
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Model {
    #[serde(rename = "model")]
    pub id: String,
    #[serde(rename = "displayName")]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "isDefault")]
    pub is_default: bool,
    #[serde(default, rename = "supportedReasoningEfforts")]
    pub efforts: Vec<Effort>,
    #[serde(default, rename = "defaultReasoningEffort")]
    pub default_effort: String,
    #[serde(default, rename = "serviceTiers")]
    pub tiers: Vec<ServiceTier>,
    #[serde(default, rename = "defaultServiceTier")]
    pub default_tier: Option<String>,
}
impl Model {
    pub fn initial_effort(&self) -> &str {
        if self.efforts.iter().any(|e| e.id == "low") {
            "low"
        } else {
            &self.default_effort
        }
    }
}
impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
fn generation(id: &str) -> Vec<u32> {
    id.strip_prefix("gpt-")
        .unwrap_or_default()
        .split('-')
        .next()
        .unwrap_or_default()
        .split('.')
        .map_while(|s| s.parse().ok())
        .collect()
}
/// The catalog has no release timestamp: choose the highest GPT generation,
/// prefer its catalog default on ties, and retain catalog order otherwise.
pub fn latest(models: &[Model]) -> Option<&Model> {
    models
        .iter()
        .enumerate()
        .max_by_key(|(i, m)| (generation(&m.id), m.is_default, std::cmp::Reverse(*i)))
        .map(|(_, m)| m)
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub auto_apply: bool,
    pub model: Option<String>,
    pub efforts: BTreeMap<String, String>,
    pub tiers: BTreeMap<String, String>,
}
impl Preferences {
    pub fn resolve<'a>(&self, models: &'a [Model]) -> Result<&'a Model, String> {
        match &self.model {
            Some(id) => models.iter().find(|m| &m.id == id).ok_or_else(|| {
                format!("{id} is no longer available. Choose a model or use Latest.")
            }),
            None => latest(models).ok_or_else(|| "No available models. Reconnect to Codex.".into()),
        }
    }
    pub fn effort<'a>(&'a self, model: &'a Model) -> &'a str {
        self.efforts
            .get(&model.id)
            .filter(|id| model.efforts.iter().any(|e| &e.id == *id))
            .map(String::as_str)
            .unwrap_or_else(|| model.initial_effort())
    }
    pub fn tier<'a>(&'a self, model: &'a Model) -> Option<&'a str> {
        self.tiers
            .get(&model.id)
            .filter(|id| *id == "default" || model.tiers.iter().any(|t| &t.id == *id))
            .map(String::as_str)
            .or(Some("default"))
    }
    pub fn path() -> Option<PathBuf> {
        crate::compatibility::data_directory()
            .ok()
            .map(|p| p.join("assistant-preferences.json"))
    }
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .filter(|b| b.len() < 65536)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("No preferences directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::storage::write_atomic(&path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn model(id: &str, default: bool) -> Model {
        serde_json::from_value(serde_json::json!({"model":id,"displayName":id,"isDefault":default,"defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"medium","description":"Balanced"}]})).unwrap()
    }
    #[test]
    fn automatic_follows_new_generation_but_explicit_choice_survives_refresh() {
        let mut models = vec![model("gpt-5.6-sol", true), model("gpt-6-astra", false)];
        let mut prefs = Preferences::default();
        assert_eq!(prefs.resolve(&models).unwrap().id, "gpt-6-astra");
        models.push(model("gpt-6.1", false));
        assert_eq!(prefs.resolve(&models).unwrap().id, "gpt-6.1");
        prefs.model = Some("gpt-5.6-sol".into());
        let persisted: Preferences =
            serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(persisted.resolve(&models).unwrap().id, "gpt-5.6-sol");
        models.remove(0);
        assert!(persisted.resolve(&models).is_err());
    }
    #[test]
    fn unsupported_effort_uses_the_models_advertised_default() {
        let model = model("gpt-6-astra", false);
        let mut prefs = Preferences::default();
        prefs.efforts.insert(model.id.clone(), "unsupported".into());
        assert_eq!(prefs.effort(&model), "medium");
        assert_eq!(latest(&[]), None);
        assert_eq!(generation("gpt-10.2-test"), vec![10, 2]);
    }
    #[test]
    fn new_preferences_use_standard_low_and_keep_explicit_overrides() {
        let mut model = model("gpt-6-astra", false);
        model.efforts.push(Effort {
            id: "low".into(),
            description: "Fast".into(),
        });
        model.tiers.push(ServiceTier {
            id: "priority".into(),
            name: "Fast".into(),
            description: "Increased usage".into(),
        });
        model.default_tier = Some("priority".into());
        let mut prefs = Preferences::default();
        assert_eq!(prefs.effort(&model), "low");
        assert_eq!(prefs.tier(&model), Some("default"));
        prefs.efforts.insert(model.id.clone(), "medium".into());
        prefs.tiers.insert(model.id.clone(), "priority".into());
        assert_eq!(prefs.effort(&model), "medium");
        assert_eq!(prefs.tier(&model), Some("priority"));
    }
}
