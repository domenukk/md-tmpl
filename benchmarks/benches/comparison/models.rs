//! Data models for comparison benchmarks.
//!
//! Includes both:
//! 1. Shared models carrying helper fields for foreign engines (Handlebars, Tera).
//! 2. Strict models matching exact frontmatter declarations (zero extra fields).

use serde::Serialize;

// ==========================================================================
// Scenario 1 — Simple
// ==========================================================================

#[derive(Serialize, Clone)]
pub struct SimpleData {
    pub name: String,
    pub place: String,
}

pub fn simple_data() -> SimpleData {
    SimpleData {
        name: "Alice".into(),
        place: "Wonderland".into(),
    }
}

// ==========================================================================
// Scenario 2 — Loop
// ==========================================================================

#[derive(Serialize, Clone)]
pub struct LoopData {
    pub items: Vec<LoopItem>,
}

#[derive(Serialize, Clone)]
pub struct LoopItem {
    pub label: String,
    pub value: i64,
}

pub fn loop_data() -> LoopData {
    LoopData {
        items: vec![
            LoopItem { label: "Alpha".into(), value: 10 },
            LoopItem { label: "Beta".into(),  value: 20 },
            LoopItem { label: "Gamma".into(), value: 30 },
        ],
    }
}

// ==========================================================================
// Scenario 3 — Conditional
// ==========================================================================

/// Shared conditional data — carries boolean flags for Handlebars.
#[derive(Serialize, Clone)]
pub struct ConditionalData {
    pub level: String,
    pub score: i64,
    /// Handlebars can't do string equality — needs boolean flags.
    pub is_high: bool,
    pub is_medium: bool,
}

pub fn conditional_data() -> ConditionalData {
    ConditionalData {
        level: "medium".into(),
        score: 75,
        is_high: false,
        is_medium: true,
    }
}

/// Strict conditional data — exact frontmatter schema (no extra boolean flags).
#[derive(Serialize, Clone)]
pub struct ConditionalStrictData {
    pub level: String,
    pub score: i64,
}

pub fn conditional_strict_data() -> ConditionalStrictData {
    ConditionalStrictData {
        level: "medium".into(),
        score: 75,
    }
}

// ==========================================================================
// Scenario 4 — Hero: nested loops + conditionals
// ==========================================================================

/// Shared hero report — includes pre-formatted strings and comparison flags.
#[derive(Serialize, Clone)]
pub struct HeroReport {
    pub title: String,
    pub sections: Vec<HeroSection>,
}

#[derive(Serialize, Clone)]
pub struct HeroSection {
    pub heading: String,
    pub entries: Vec<HeroEntry>,
}

#[derive(Serialize, Clone)]
pub struct HeroEntry {
    pub name: String,
    pub active: bool,
    pub score: f64,
    /// Pre-formatted score for engines without `fixed()` filter.
    pub score_fmt: String,
    /// Pre-computed flag for engines without numeric comparison.
    pub has_positive_score: bool,
    pub tags: Vec<HeroTag>,
}

#[derive(Serialize, Clone)]
pub struct HeroTag {
    pub label: String,
}

impl HeroEntry {
    pub fn new(name: &str, active: bool, score: f64, tags: &[&str]) -> Self {
        Self {
            name: name.into(),
            active,
            score,
            score_fmt: format!("{score:.1}"),
            has_positive_score: score > 0.0,
            tags: tags.iter().map(|t| HeroTag { label: t.to_string() }).collect(),
        }
    }
}

pub fn hero_data() -> HeroReport {
    HeroReport {
        title: "System Report".into(),
        sections: vec![
            HeroSection {
                heading: "Overview".into(),
                entries: vec![
                    HeroEntry::new("Service-A", true, 98.7, &["prod", "critical"]),
                    HeroEntry::new("Service-B", false, 45.2, &["staging"]),
                    HeroEntry::new("Service-C", false, 0.0, &["deprecated"]),
                ],
            },
            HeroSection {
                heading: "Metrics".into(),
                entries: vec![
                    HeroEntry::new("Latency", true, 12.3, &["p99"]),
                    HeroEntry::new("Throughput", false, 0.0, &["batch"]),
                ],
            },
        ],
    }
}

/// Strict hero report — exact schema with only declared fields.
#[derive(Serialize, Clone)]
pub struct HeroStrictReport {
    pub title: String,
    pub sections: Vec<HeroStrictSection>,
}

#[derive(Serialize, Clone)]
pub struct HeroStrictSection {
    pub heading: String,
    pub entries: Vec<HeroStrictEntry>,
}

#[derive(Serialize, Clone)]
pub struct HeroStrictEntry {
    pub name: String,
    pub active: bool,
    pub score: f64,
    pub tags: Vec<HeroStrictTag>,
}

#[derive(Serialize, Clone)]
pub struct HeroStrictTag {
    pub label: String,
}

impl HeroStrictEntry {
    pub fn new(name: &str, active: bool, score: f64, tags: &[&str]) -> Self {
        Self {
            name: name.into(),
            active,
            score,
            tags: tags.iter().map(|t| HeroStrictTag { label: t.to_string() }).collect(),
        }
    }
}

pub fn hero_strict_data() -> HeroStrictReport {
    HeroStrictReport {
        title: "System Report".into(),
        sections: vec![
            HeroStrictSection {
                heading: "Overview".into(),
                entries: vec![
                    HeroStrictEntry::new("Service-A", true, 98.7, &["prod", "critical"]),
                    HeroStrictEntry::new("Service-B", false, 45.2, &["staging"]),
                    HeroStrictEntry::new("Service-C", false, 0.0, &["deprecated"]),
                ],
            },
            HeroStrictSection {
                heading: "Metrics".into(),
                entries: vec![
                    HeroStrictEntry::new("Latency", true, 12.3, &["p99"]),
                    HeroStrictEntry::new("Throughput", false, 0.0, &["batch"]),
                ],
            },
        ],
    }
}

// ==========================================================================
// Scenario 5 — Mega: large data, deep nesting, idx, filters
// ==========================================================================

/// Shared mega report — used by all engines via serde.
#[derive(Serialize, Clone)]
pub struct MegaReport {
    pub org: String,
    pub teams: Vec<MegaTeam>,
}

#[derive(Serialize, Clone)]
pub struct MegaTeam {
    pub name: String,
    pub lead: String,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idx: Option<usize>,
    pub members: Vec<MegaMember>,
}

#[derive(Serialize, Clone)]
pub struct MegaMember {
    pub name: String,
    pub role: String,
    pub score: f64,
    /// Pre-formatted score for engines without `fixed()` filter.
    pub score_fmt: String,
    /// Pre-computed rating string for engines without numeric comparison.
    pub rating: String,
    /// Handlebars-friendly boolean flags.
    pub is_outstanding: bool,
    pub is_good: bool,
    pub is_average: bool,
    pub skills: Vec<MegaSkill>,
}

#[derive(Serialize, Clone)]
pub struct MegaSkill {
    pub name: String,
}

impl MegaMember {
    pub fn new(name: &str, role: &str, score: f64, skills: &[&str]) -> Self {
        let rating = if score > 90.0 {
            "outstanding"
        } else if score > 70.0 {
            "good"
        } else if score > 50.0 {
            "average"
        } else {
            "needs_improvement"
        };
        Self {
            name: name.into(),
            role: role.into(),
            score,
            score_fmt: format!("{score:.1}"),
            rating: rating.into(),
            is_outstanding: rating == "outstanding",
            is_good: rating == "good",
            is_average: rating == "average",
            skills: skills.iter().map(|s| MegaSkill { name: s.to_string() }).collect(),
        }
    }
}

impl MegaTeam {
    pub fn new(name: &str, lead: &str, active: bool, idx: usize, members: Vec<MegaMember>) -> Self {
        Self {
            name: name.into(),
            lead: lead.into(),
            active,
            idx: Some(idx),
            members,
        }
    }
}

pub fn mega_data() -> MegaReport {
    MegaReport {
        org: "Acme Corp".into(),
        teams: vec![
            MegaTeam::new("Backend", "Alice", true, 1, vec![
                MegaMember::new("Bob", "Senior", 95.5, &["Rust", "Go", "SQL"]),
                MegaMember::new("Carol", "Mid", 78.3, &["Python", "Docker"]),
                MegaMember::new("Dave", "Junior", 62.1, &["JavaScript", "HTML"]),
                MegaMember::new("Eve", "Senior", 91.0, &["Java", "Kotlin", "gRPC"]),
                MegaMember::new("Frank", "Intern", 45.0, &["Python"]),
            ]),
            MegaTeam::new("Frontend", "Grace", true, 2, vec![
                MegaMember::new("Heidi", "Senior", 88.7, &["React", "TypeScript", "CSS"]),
                MegaMember::new("Ivan", "Mid", 71.2, &["Vue", "JavaScript"]),
                MegaMember::new("Judy", "Junior", 55.0, &["HTML", "CSS"]),
                MegaMember::new("Karl", "Senior", 92.4, &["Angular", "RxJS", "SCSS"]),
                MegaMember::new("Liam", "Mid", 68.9, &["Svelte"]),
            ]),
            MegaTeam::new("SRE", "Mallory", false, 3, vec![
                MegaMember::new("Nancy", "Senior", 97.1, &["Kubernetes", "Terraform", "Go"]),
                MegaMember::new("Oscar", "Mid", 73.5, &["Ansible", "Bash"]),
                MegaMember::new("Peggy", "Junior", 51.2, &["Linux"]),
                MegaMember::new("Quentin", "Senior", 89.3, &["Prometheus", "Grafana"]),
                MegaMember::new("Ruth", "Intern", 38.0, &["Python"]),
            ]),
        ],
    }
}

/// Strict mega report — exact schema with only declared fields.
#[derive(Serialize, Clone)]
pub struct MegaStrictReport {
    pub org: String,
    pub teams: Vec<MegaStrictTeam>,
}

#[derive(Serialize, Clone)]
pub struct MegaStrictTeam {
    pub name: String,
    pub lead: String,
    pub active: bool,
    pub idx: usize,
    pub members: Vec<MegaStrictMember>,
}

#[derive(Serialize, Clone)]
pub struct MegaStrictMember {
    pub name: String,
    pub role: String,
    pub score: f64,
    pub skills: Vec<MegaStrictSkill>,
}

#[derive(Serialize, Clone)]
pub struct MegaStrictSkill {
    pub name: String,
}

impl MegaStrictMember {
    pub fn new(name: &str, role: &str, score: f64, skills: &[&str]) -> Self {
        Self {
            name: name.into(),
            role: role.into(),
            score,
            skills: skills.iter().map(|s| MegaStrictSkill { name: s.to_string() }).collect(),
        }
    }
}

impl MegaStrictTeam {
    pub fn new(name: &str, lead: &str, active: bool, idx: usize, members: Vec<MegaStrictMember>) -> Self {
        Self {
            name: name.into(),
            lead: lead.into(),
            active,
            idx,
            members,
        }
    }
}

pub fn mega_strict_data() -> MegaStrictReport {
    MegaStrictReport {
        org: "Acme Corp".into(),
        teams: vec![
            MegaStrictTeam::new("Backend", "Alice", true, 1, vec![
                MegaStrictMember::new("Bob", "Senior", 95.5, &["Rust", "Go", "SQL"]),
                MegaStrictMember::new("Carol", "Mid", 78.3, &["Python", "Docker"]),
                MegaStrictMember::new("Dave", "Junior", 62.1, &["JavaScript", "HTML"]),
                MegaStrictMember::new("Eve", "Senior", 91.0, &["Java", "Kotlin", "gRPC"]),
                MegaStrictMember::new("Frank", "Intern", 45.0, &["Python"]),
            ]),
            MegaStrictTeam::new("Frontend", "Grace", true, 2, vec![
                MegaStrictMember::new("Heidi", "Senior", 88.7, &["React", "TypeScript", "CSS"]),
                MegaStrictMember::new("Ivan", "Mid", 71.2, &["Vue", "JavaScript"]),
                MegaStrictMember::new("Judy", "Junior", 55.0, &["HTML", "CSS"]),
                MegaStrictMember::new("Karl", "Senior", 92.4, &["Angular", "RxJS", "SCSS"]),
                MegaStrictMember::new("Liam", "Mid", 68.9, &["Svelte"]),
            ]),
            MegaStrictTeam::new("SRE", "Mallory", false, 3, vec![
                MegaStrictMember::new("Nancy", "Senior", 97.1, &["Kubernetes", "Terraform", "Go"]),
                MegaStrictMember::new("Oscar", "Mid", 73.5, &["Ansible", "Bash"]),
                MegaStrictMember::new("Peggy", "Junior", 51.2, &["Linux"]),
                MegaStrictMember::new("Quentin", "Senior", 89.3, &["Prometheus", "Grafana"]),
                MegaStrictMember::new("Ruth", "Intern", 38.0, &["Python"]),
            ]),
        ],
    }
}
