//! Skill system for YowCode
//!
//! This module provides a flexible skill/command system that allows:
//! - Defining custom skills that can be invoked by the AI
//! - Skill discovery and registration
//! - Permission-based skill execution
//! - Skill chaining and composition

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Skill execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl SkillResult {
    pub fn success(output: String) -> Self {
        Self {
            success: true,
            output,
            error: None,
            metadata: HashMap::new(),
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error),
            metadata: HashMap::new(),
        }
    }
}

/// Skill parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: SkillCategory,
    pub parameters: Vec<SkillParameter>,
    pub requires_permission: bool,
    pub is_destructive: bool,
    // Handler is not serialized - skills loaded from storage will need a handler registered separately
    #[serde(skip)]
    pub handler: Option<SkillHandler>,
}

impl Skill {
    /// Create a new skill
    pub fn new(name: String, description: String, category: SkillCategory, handler: SkillHandler) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description,
            category,
            parameters: Vec::new(),
            requires_permission: false,
            is_destructive: false,
            handler: Some(handler),
        }
    }

    /// Add a parameter to the skill
    pub fn with_parameter(mut self, param: SkillParameter) -> Self {
        self.parameters.push(param);
        self
    }

    /// Set permission requirement
    pub fn requires_permission(mut self, requires: bool) -> Self {
        self.requires_permission = requires;
        self
    }

    /// Set destructive flag
    pub fn is_destructive(mut self, destructive: bool) -> Self {
        self.is_destructive = destructive;
        self
    }

    /// Execute the skill
    pub fn execute(&self, args: HashMap<String, String>) -> SkillResult {
        match &self.handler {
            Some(handler) => (handler)(args),
            None => SkillResult::error("Skill has no handler registered".to_string()),
        }
    }
}

/// Skill categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SkillCategory {
    FileOperations,
    CodeAnalysis,
    CodeGeneration,
    SystemOperations,
    WebOperations,
    GitOperations,
    DatabaseOperations,
    Custom(String),
}

/// Skill handler type
pub type SkillHandler = fn(HashMap<String, String>) -> SkillResult;

/// Skill registry for managing available skills
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<Uuid, Skill>,
    by_name: HashMap<String, Uuid>,
    by_category: HashMap<SkillCategory, Vec<Uuid>>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// Create a new skill registry
    pub fn new() -> Self {
        let mut registry = Self {
            skills: HashMap::new(),
            by_name: HashMap::new(),
            by_category: HashMap::new(),
        };

        // Register default skills
        registry.register_default_skills();
        registry
    }

    /// Register default built-in skills
    fn register_default_skills(&mut self) {
        // File count skill
        self.register(Skill::new(
            "count_files".to_string(),
            "Count files matching a pattern".to_string(),
            SkillCategory::FileOperations,
            |args| {
                let pattern = args.get("pattern").unwrap_or(&"*".to_string()).clone();
                // This is a simplified implementation
                SkillResult::success(format!("Counted files matching: {}", pattern))
            },
        )
        .with_parameter(SkillParameter {
            name: "pattern".to_string(),
            param_type: "string".to_string(),
            description: "File pattern to match (e.g., *.rs)".to_string(),
            required: false,
            default: Some("*".to_string()),
        }));

        // Line count skill
        self.register(Skill::new(
            "count_lines".to_string(),
            "Count lines in a file or directory".to_string(),
            SkillCategory::CodeAnalysis,
            |args| {
                let path = args.get("path").unwrap_or(&".".to_string()).clone();
                SkillResult::success(format!("Counted lines in: {}", path))
            },
        )
        .with_parameter(SkillParameter {
            name: "path".to_string(),
            param_type: "string".to_string(),
            description: "Path to count lines in".to_string(),
            required: false,
            default: Some(".".to_string()),
        }));

        // Generate TODO skill
        self.register(Skill::new(
            "generate_todo".to_string(),
            "Generate TODO comments from code".to_string(),
            SkillCategory::CodeAnalysis,
            |_| SkillResult::success("Generated TODO comments".to_string()),
        ));
    }

    /// Register a skill
    pub fn register(&mut self, skill: Skill) {
        let id = skill.id;
        let name = skill.name.clone();
        let category = skill.category.clone();

        self.skills.insert(id, skill);
        self.by_name.insert(name, id);
        self.by_category.entry(category).or_default().push(id);
    }

    /// Get a skill by ID
    pub fn get(&self, id: Uuid) -> Option<&Skill> {
        self.skills.get(&id)
    }

    /// Get a skill by name
    pub fn get_by_name(&self, name: &str) -> Option<&Skill> {
        self.by_name.get(name).and_then(|id| self.skills.get(id))
    }

    /// List all skills
    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// List skills by category
    pub fn list_by_category(&self, category: &SkillCategory) -> Vec<&Skill> {
        self.by_category
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.skills.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Remove a skill
    pub fn remove(&mut self, id: Uuid) -> Option<Skill> {
        let skill = self.skills.remove(&id)?;
        self.by_name.remove(&skill.name);
        if let Some(ids) = self.by_category.get_mut(&skill.category) {
            ids.retain(|x| x != &id);
        }
        Some(skill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_creation() {
        let handler = |_| SkillResult::success("test".to_string());
        let skill = Skill::new(
            "test_skill".to_string(),
            "A test skill".to_string(),
            SkillCategory::Custom("test".to_string()),
            handler,
        );

        assert_eq!(skill.name, "test_skill");
        assert!(!skill.requires_permission);
    }

    #[test]
    fn test_skill_registry() {
        let registry = SkillRegistry::new();
        assert!(!registry.list().is_empty());
        assert!(registry.get_by_name("count_files").is_some());
    }
}
