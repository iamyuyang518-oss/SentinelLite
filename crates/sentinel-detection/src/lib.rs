//! Explainable, conservative process-behavior analysis.
//!
//! This crate does not decide whether a process is malicious. It reports
//! observable patterns, the data they were derived from, and how missing or
//! incomplete data limits the conclusion. Every result is traceable to stored
//! event IDs and carries a rule version.

pub mod analysis;

pub use analysis::{
    AnalysisConfig, Confidence, DetectionContext, DetectionEvidence, DetectionResult,
    DetectionStatus, InputQuality, RULES_VERSION, RuleDefinition, SensorHealthInput,
    SessionAnalysis, SessionDataInput, Severity, ThresholdValue, analyze, rule_catalog,
};
