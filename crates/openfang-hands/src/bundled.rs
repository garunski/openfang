//! Compile-time embedded Hand definitions.

use crate::{parse_hand_toml, HandDefinition, HandError};

/// Returns all bundled hand definitions as (id, HAND.toml content, SKILL.md content).
pub fn bundled_hands() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![(
        "workflow-coordinator",
        include_str!("../bundled/workflow-coordinator/HAND.toml"),
        include_str!("../bundled/workflow-coordinator/SKILL.md"),
    )]
}

/// Parse a bundled HAND.toml into a HandDefinition with its skill content attached.
pub fn parse_bundled(
    _id: &str,
    toml_content: &str,
    skill_content: &str,
) -> Result<HandDefinition, HandError> {
    let mut def: HandDefinition =
        parse_hand_toml(toml_content).map_err(|e| HandError::TomlParse(e.to_string()))?;
    if !skill_content.is_empty() {
        def.skill_content = Some(skill_content.to_string());
    }
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_hands_non_empty() {
        let hands = bundled_hands();
        assert_eq!(hands.len(), 1);
        assert_eq!(hands[0].0, "workflow-coordinator");
    }

    #[test]
    fn bundled_hands_parse() {
        let hands = bundled_hands();
        let def = parse_bundled(hands[0].0, hands[0].1, hands[0].2).unwrap();
        assert_eq!(def.id, "workflow-coordinator");
        assert!(def.tools.contains(&"start_project_workflow".to_string()));
    }
}
