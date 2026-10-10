//! Suffix applicability and ordered rule metadata from OPSIN SuffixRules.
use crate::{InitializationError, resources};
use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuffixRuleType {
    AddGroup,
    AddSuffixPrefixIfNonePresentAndCyclic,
    SetOutAtom,
    ChangeCharge,
    AddFunctionalAtomsToHydroxyGroups,
    ChargeHydroxyGroups,
    RemoveTerminalOxygen,
    ConvertHydroxyGroupsToOutAtoms,
    ConvertHydroxyGroupsToPositiveCharge,
    SetAcidicElement,
}

impl SuffixRuleType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddGroup => "addgroup",
            Self::AddSuffixPrefixIfNonePresentAndCyclic => "addSuffixPrefixIfNonePresentAndCyclic",
            Self::SetOutAtom => "setOutAtom",
            Self::ChangeCharge => "changecharge",
            Self::AddFunctionalAtomsToHydroxyGroups => "addFunctionalAtomsToHydroxyGroups",
            Self::ChargeHydroxyGroups => "chargeHydroxyGroups",
            Self::RemoveTerminalOxygen => "removeTerminalOxygen",
            Self::ConvertHydroxyGroupsToOutAtoms => "convertHydroxyGroupsToOutAtoms",
            Self::ConvertHydroxyGroupsToPositiveCharge => "convertHydroxyGroupsToPositiveCharge",
            Self::SetAcidicElement => "setAcidicElement",
        }
    }
    fn from_str(value: &str) -> Option<Self> {
        [
            Self::AddGroup,
            Self::AddSuffixPrefixIfNonePresentAndCyclic,
            Self::SetOutAtom,
            Self::ChangeCharge,
            Self::AddFunctionalAtomsToHydroxyGroups,
            Self::ChargeHydroxyGroups,
            Self::RemoveTerminalOxygen,
            Self::ConvertHydroxyGroupsToOutAtoms,
            Self::ConvertHydroxyGroupsToPositiveCharge,
            Self::SetAcidicElement,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuffixRule {
    pub kind: SuffixRuleType,
    pub attributes: Vec<(String, String)>,
}
impl SuffixRule {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone)]
struct ApplicableSuffix {
    required_sub_type: Option<String>,
    rules: Arc<[SuffixRule]>,
}

#[derive(Debug, Clone)]
pub struct SuffixRules {
    applicability: BTreeMap<String, BTreeMap<String, Vec<ApplicableSuffix>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuffixRuleError(pub String);
impl fmt::Display for SuffixRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SuffixRuleError {}

impl SuffixRules {
    pub fn new() -> Result<Self, InitializationError> {
        let document = resources::document("suffixRules.xml")?;
        let mut rule_map: BTreeMap<String, Arc<[SuffixRule]>> = BTreeMap::new();
        for rule in document.descendants().filter(|n| n.has_tag_name("rule")) {
            let value = resources::required(rule, "value")?;
            let mut rules = Vec::new();
            for tag in rule.children().filter(roxmltree::Node::is_element) {
                let kind = SuffixRuleType::from_str(tag.tag_name().name()).ok_or_else(|| {
                    InitializationError(format!(
                        "Unknown suffix rule tag {}",
                        tag.tag_name().name()
                    ))
                })?;
                rules.push(SuffixRule {
                    kind,
                    attributes: tag
                        .attributes()
                        .map(|attribute| (attribute.name().into(), attribute.value().into()))
                        .collect(),
                });
            }
            if rule_map.insert(value.into(), rules.into()).is_some() {
                return Err(InitializationError(format!(
                    "Suffix: {value} appears multiple times in suffixRules.xml"
                )));
            }
        }
        let mut applicability = BTreeMap::new();
        let document = resources::document("suffixApplicability.xml")?;
        for group in document
            .descendants()
            .filter(|n| n.has_tag_name("groupType"))
        {
            let mut suffixes: BTreeMap<String, Vec<ApplicableSuffix>> = BTreeMap::new();
            for suffix in group.children().filter(|n| n.has_tag_name("suffix")) {
                let value = resources::required(suffix, "value")?;
                let name = suffix.text().unwrap_or_default();
                let rules = rule_map.get(name).ok_or_else(|| {
                    InitializationError(format!(
                        "Suffix: {name} does not have a rule associated with it in suffixRules.xml"
                    ))
                })?;
                suffixes
                    .entry(value.into())
                    .or_default()
                    .push(ApplicableSuffix {
                        required_sub_type: suffix.attribute("subType").map(str::to_owned),
                        rules: rules.clone(),
                    });
            }
            applicability.insert(resources::required(group, "type")?.into(), suffixes);
        }
        Ok(Self { applicability })
    }

    pub fn is_group_type_with_specific_suffix_rules(&self, group_type: &str) -> bool {
        self.applicability.contains_key(group_type)
    }

    pub fn rule_tags(
        &self,
        group_type: &str,
        suffix_value: &str,
        sub_type: Option<&str>,
    ) -> Result<&[SuffixRule], SuffixRuleError> {
        let group = self.applicability.get(group_type).ok_or_else(|| SuffixRuleError(format!(
            "Suffix Type: {group_type} does not have a corresponding groupType entry in suffixApplicability.xml")))?;
        let candidates = group.get(suffix_value).filter(|candidates| !candidates.is_empty()).ok_or_else(|| SuffixRuleError(format!(
            "Suffix: {suffix_value} does not apply to the group it was associated with (type: {group_type}) according to suffixApplicability.xml")))?;
        let mut matching = None;
        for candidate in candidates {
            if candidate
                .required_sub_type
                .as_deref()
                .is_some_and(|required| Some(required) != sub_type)
            {
                continue;
            }
            if matching.is_some() {
                return Err(SuffixRuleError(format!(
                    "Suffix: {suffix_value} appears multiple times in suffixApplicability.xml"
                )));
            }
            matching = Some(candidate.rules.as_ref());
        }
        matching.ok_or_else(|| SuffixRuleError(format!(
            "Suffix: {suffix_value} does not apply to the group it was associated with (type: {group_type}) due to the group's subType: {} according to suffixApplicability.xml",
            sub_type.unwrap_or("null"))))
    }
}
