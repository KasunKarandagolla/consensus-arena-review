//! Dynamic work-graph contracts layered above Arena's deterministic authority kernel.
//!
//! The controller owns admission, epochs, budgets and authority. Models may
//! propose child work, but only Arena creates durable child work orders.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_DELEGATION_DEPTH: u8 = 3;
pub const MAX_DELEGATED_CHILDREN: usize = 12;
pub const MAX_RESEARCH_CYCLES: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchChannel {
    Web,
    Github,
    Youtube,
    Reddit,
    X,
    Rss,
    ResearchPapers,
    Douyin,
    Tiktok,
}

impl ResearchChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Github => "github",
            Self::Youtube => "youtube",
            Self::Reddit => "reddit",
            Self::X => "x",
            Self::Rss => "rss",
            Self::ResearchPapers => "research_papers",
            Self::Douyin => "douyin",
            Self::Tiktok => "tiktok",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchDepth {
    Standard,
    Deep,
    Exhaustive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerDirectiveKind {
    Research,
    ScopeChange,
    Constraint,
    Idea,
    GeneralGuidance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerDirective {
    pub directive_id: String,
    pub kind: OwnerDirectiveKind,
    pub text: String,
    pub requested_channels: Vec<ResearchChannel>,
    pub research_depth: Option<ResearchDepth>,
    pub must_complete_before_decision: bool,
    pub admitted_revision: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMandateStatus {
    Pending,
    Running,
    Satisfied,
    PartiallyUnavailable,
    Blocked,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchMandate {
    pub mandate_id: String,
    pub directive_id: String,
    pub topic: String,
    pub channels: Vec<ResearchChannel>,
    pub mandatory_channels: Vec<ResearchChannel>,
    pub depth: ResearchDepth,
    pub must_complete_before_decision: bool,
    pub minimum_distinct_sources: u16,
    pub max_cycles: u16,
    pub status: ResearchMandateStatus,
    #[serde(default)]
    pub cycles_completed: u16,
    pub child_work_order_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub completed_channels: Vec<ResearchChannel>,
    pub unavailable_channels: Vec<ResearchChannel>,
}

impl ResearchMandate {
    pub fn validate(&self) -> Result<(), String> {
        if self.mandate_id.trim().is_empty()
            || self.directive_id.trim().is_empty()
            || self.topic.trim().is_empty()
            || self.max_cycles == 0
            || self.max_cycles > MAX_RESEARCH_CYCLES
            || self.minimum_distinct_sources == 0
        {
            return Err("research mandate is missing bounded identity, scope, or budget".to_string());
        }
        if self
            .mandatory_channels
            .iter()
            .any(|channel| !self.channels.contains(channel))
        {
            return Err("mandatory research channels must be included in the campaign".to_string());
        }
        Ok(())
    }

    pub fn can_advance(&self) -> bool {
        if !self.must_complete_before_decision {
            return true;
        }
        self.status == ResearchMandateStatus::Satisfied
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedResearchTask {
    pub channel: ResearchChannel,
    pub question: String,
    #[serde(default)]
    pub model_id: Option<String>,
}

impl DelegatedResearchTask {
    pub fn validate(&self) -> Result<(), String> {
        if self.question.trim().is_empty() || self.question.len() > 4_000 {
            return Err("delegated research question is empty or oversized".to_string());
        }
        if let Some(model) = self.model_id.as_deref() {
            crate::opencode_adapter::validate_model_identifier(model)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchLeadPlan {
    pub complete: bool,
    pub completion_reason: String,
    pub tasks: Vec<DelegatedResearchTask>,
    pub follow_up_focus: Option<String>,
}

impl ResearchLeadPlan {
    pub fn validate(&self) -> Result<(), String> {
        if self.tasks.len() > MAX_DELEGATED_CHILDREN {
            return Err("research lead proposed too many child specialists".to_string());
        }
        if self.complete && self.completion_reason.trim().is_empty() {
            return Err("completed research plan requires a completion reason".to_string());
        }
        if !self.complete && self.tasks.is_empty() {
            return Err("incomplete research plan must propose bounded follow-up work".to_string());
        }
        for task in &self.tasks {
            task.validate()?;
        }
        Ok(())
    }
}

fn mentions_any(lower: &str, values: &[&str]) -> bool {
    values.iter().any(|value| lower.contains(value))
}

pub fn explicit_research_channels(text: &str) -> Vec<ResearchChannel> {
    let lower = text.to_ascii_lowercase();
    let mut channels = BTreeSet::new();
    if mentions_any(&lower, &["github", "open source", "opensource", "repository", "repos"]) {
        channels.insert(ResearchChannel::Github);
    }
    if mentions_any(&lower, &["youtube", "you tube"]) {
        channels.insert(ResearchChannel::Youtube);
    }
    if lower.contains("reddit") {
        channels.insert(ResearchChannel::Reddit);
    }
    if mentions_any(&lower, &["tiktok", "tik tok"]) {
        channels.insert(ResearchChannel::Tiktok);
    }
    if lower.contains("douyin") {
        channels.insert(ResearchChannel::Douyin);
    }
    if mentions_any(&lower, &["twitter", " x ", "x.com"]) {
        channels.insert(ResearchChannel::X);
    }
    if lower.contains("rss") {
        channels.insert(ResearchChannel::Rss);
    }
    if mentions_any(
        &lower,
        &[
            "research paper",
            "research papers",
            "academic paper",
            "academic papers",
            "arxiv",
            "scholar",
        ],
    ) {
        channels.insert(ResearchChannel::ResearchPapers);
    }
    if mentions_any(
        &lower,
        &["web search", "normal web", "websites", "internet", "online research"],
    ) {
        channels.insert(ResearchChannel::Web);
    }
    channels.into_iter().collect()
}

pub fn owner_directive_from_text(
    text: &str,
    admitted_revision: u64,
    created_at: i64,
) -> Result<OwnerDirective, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 16 * 1024 || text.chars().any(char::is_control) {
        return Err("owner guidance is empty, oversized, or contains invalid control text".to_string());
    }
    let lower = text.to_ascii_lowercase();
    let channels = explicit_research_channels(text);
    let research_requested = !channels.is_empty()
        || mentions_any(
            &lower,
            &[
                "research",
                "investigate",
                "deep dive",
                "look into",
                "prior art",
                "competitor",
                "market study",
            ],
        );
    let kind = if research_requested {
        OwnerDirectiveKind::Research
    } else if mentions_any(&lower, &["must ", "constraint", "do not", "never "]) {
        OwnerDirectiveKind::Constraint
    } else if mentions_any(&lower, &["change scope", "also add", "remove ", "instead"]) {
        OwnerDirectiveKind::ScopeChange
    } else {
        OwnerDirectiveKind::GeneralGuidance
    };
    let research_depth = research_requested.then_some(if mentions_any(
        &lower,
        &["exhaustive", "as much as needed", "thorough", "comprehensive"],
    ) {
        ResearchDepth::Exhaustive
    } else if mentions_any(&lower, &["deep", "proper research", "properly research", "serious research"]) {
        ResearchDepth::Deep
    } else {
        ResearchDepth::Standard
    });
    let must_complete_before_decision = research_requested
        && mentions_any(
            &lower,
            &[
                "research first",
                "first research",
                "before proposal",
                "before deciding",
                "before decision",
                "before build",
                "before implement",
                "proper research first",
            ],
        );
    Ok(OwnerDirective {
        directive_id: format!("owner-directive:{}", uuid::Uuid::new_v4()),
        kind,
        text: text.to_string(),
        requested_channels: channels,
        research_depth,
        must_complete_before_decision,
        admitted_revision,
        created_at,
    })
}

fn env_model(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name).ok().filter(|value| !value.trim().is_empty()) {
        Some(value) => crate::opencode_adapter::validate_model_identifier(&value).map(Some),
        None => Ok(None),
    }
}

pub fn configured_research_lead_model() -> Result<Option<String>, String> {
    env_model("ARENA_MODEL_RESEARCH_LEAD")
}

pub fn configured_channel_model(channel: ResearchChannel) -> Result<Option<String>, String> {
    let key = match channel {
        ResearchChannel::Web => "ARENA_MODEL_RESEARCH_WEB",
        ResearchChannel::Github => "ARENA_MODEL_RESEARCH_GITHUB",
        ResearchChannel::Youtube => "ARENA_MODEL_RESEARCH_YOUTUBE",
        ResearchChannel::Reddit => "ARENA_MODEL_RESEARCH_REDDIT",
        ResearchChannel::X => "ARENA_MODEL_RESEARCH_X",
        ResearchChannel::Rss => "ARENA_MODEL_RESEARCH_RSS",
        ResearchChannel::ResearchPapers => "ARENA_MODEL_RESEARCH_PAPERS",
        ResearchChannel::Douyin => "ARENA_MODEL_RESEARCH_DOUYIN",
        ResearchChannel::Tiktok => "ARENA_MODEL_RESEARCH_TIKTOK",
    };
    env_model(key)
}

pub fn research_mandate_for(directive: &OwnerDirective) -> Option<ResearchMandate> {
    if directive.kind != OwnerDirectiveKind::Research {
        return None;
    }
    let depth = directive.research_depth.unwrap_or(ResearchDepth::Standard);
    let channels = if directive.requested_channels.is_empty() {
        vec![ResearchChannel::Web, ResearchChannel::Github]
    } else {
        directive.requested_channels.clone()
    };
    let (minimum_distinct_sources, max_cycles) = match depth {
        ResearchDepth::Standard => (4, 4),
        ResearchDepth::Deep => (8, 12),
        ResearchDepth::Exhaustive => (12, MAX_RESEARCH_CYCLES),
    };
    let mandate = ResearchMandate {
        mandate_id: format!("research-mandate:{}", uuid::Uuid::new_v4()),
        directive_id: directive.directive_id.clone(),
        topic: directive.text.clone(),
        mandatory_channels: directive.requested_channels.clone(),
        channels,
        depth,
        must_complete_before_decision: directive.must_complete_before_decision,
        minimum_distinct_sources,
        max_cycles,
        status: ResearchMandateStatus::Pending,
        cycles_completed: 0,
        child_work_order_ids: Vec::new(),
        evidence_ids: Vec::new(),
        completed_channels: Vec::new(),
        unavailable_channels: Vec::new(),
    };
    mandate.validate().ok().map(|_| mandate)
}

pub fn default_new_product_research_mandate(
    directive: &OwnerDirective,
) -> ResearchMandate {
    ResearchMandate {
        mandate_id: format!("research-mandate:{}", uuid::Uuid::new_v4()),
        directive_id: directive.directive_id.clone(),
        topic: format!(
            "Establish decision-critical problem, alternatives, reuse/prior-art, and technical evidence for: {}",
            directive.text
        ),
        channels: vec![ResearchChannel::Web, ResearchChannel::Github],
        mandatory_channels: Vec::new(),
        depth: ResearchDepth::Standard,
        must_complete_before_decision: true,
        minimum_distinct_sources: 4,
        max_cycles: 4,
        status: ResearchMandateStatus::Pending,
        cycles_completed: 0,
        child_work_order_ids: Vec::new(),
        evidence_ids: Vec::new(),
        completed_channels: Vec::new(),
        unavailable_channels: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_owner_platforms_become_mandatory_channels() {
        let directive = owner_directive_from_text(
            "Do a proper research on usable open source projects in YouTube, TikTok, GitHub and Reddit before proposal.",
            4,
            10,
        )
        .expect("directive");
        assert_eq!(directive.kind, OwnerDirectiveKind::Research);
        assert!(directive.requested_channels.contains(&ResearchChannel::Youtube));
        assert!(directive.requested_channels.contains(&ResearchChannel::Tiktok));
        assert!(directive.requested_channels.contains(&ResearchChannel::Github));
        assert!(directive.requested_channels.contains(&ResearchChannel::Reddit));
        assert!(directive.must_complete_before_decision);
        let mandate = research_mandate_for(&directive).expect("mandate");
        assert_eq!(mandate.mandatory_channels, directive.requested_channels);
        assert_eq!(mandate.depth, ResearchDepth::Deep);
    }

    #[test]
    fn generic_deep_research_gets_budget_without_forcing_named_platforms() {
        let directive = owner_directive_from_text(
            "Research this product thoroughly before deciding whether to build it.",
            1,
            1,
        )
        .expect("directive");
        let mandate = research_mandate_for(&directive).expect("mandate");
        assert_eq!(mandate.depth, ResearchDepth::Exhaustive);
        assert!(mandate.channels.contains(&ResearchChannel::Web));
        assert!(mandate.channels.contains(&ResearchChannel::Github));
        assert!(mandate.mandatory_channels.is_empty());
    }

    #[test]
    fn delegation_is_bounded_and_models_are_validated() {
        let plan = ResearchLeadPlan {
            complete: false,
            completion_reason: String::new(),
            tasks: vec![DelegatedResearchTask {
                channel: ResearchChannel::Github,
                question: "Find reusable OSS projects".to_string(),
                model_id: Some("nvidia/nemotron-3-super-120b-a12b".to_string()),
            }],
            follow_up_focus: None,
        };
        assert!(plan.validate().is_ok());
    }
}
