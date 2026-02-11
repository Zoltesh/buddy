use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WarningSeverity {
    Info,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Warning {
    pub code: String,
    pub message: String,
    pub severity: WarningSeverity,
}

/// Collects and manages non-fatal warnings.
pub struct WarningCollector {
    warnings: Vec<Warning>,
}

impl WarningCollector {
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    pub fn add(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }

    pub fn list(&self) -> &[Warning] {
        &self.warnings
    }

    pub fn clear(&mut self, code: &str) {
        self.warnings.retain(|w| w.code != code);
    }
}

/// Thread-safe warning collector for use in AppState.
pub type SharedWarnings = Arc<RwLock<WarningCollector>>;

pub fn new_shared_warnings() -> SharedWarnings {
    Arc::new(RwLock::new(WarningCollector::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_list_warnings() {
        let mut collector = WarningCollector::new();
        collector.add(Warning {
            code: "test_code".into(),
            message: "test message".into(),
            severity: WarningSeverity::Warning,
        });

        assert_eq!(collector.list().len(), 1);
        assert_eq!(collector.list()[0].code, "test_code");
    }

    #[test]
    fn clear_removes_by_code() {
        let mut collector = WarningCollector::new();
        collector.add(Warning {
            code: "a".into(),
            message: "first".into(),
            severity: WarningSeverity::Warning,
        });
        collector.add(Warning {
            code: "b".into(),
            message: "second".into(),
            severity: WarningSeverity::Info,
        });

        collector.clear("a");
        assert_eq!(collector.list().len(), 1);
        assert_eq!(collector.list()[0].code, "b");
    }

    #[test]
    fn clear_nonexistent_code_is_noop() {
        let mut collector = WarningCollector::new();
        collector.add(Warning {
            code: "a".into(),
            message: "first".into(),
            severity: WarningSeverity::Warning,
        });

        collector.clear("nonexistent");
        assert_eq!(collector.list().len(), 1);
    }
}
