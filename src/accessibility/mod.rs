use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    pub role: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn get_ui_tree() -> Result<Vec<UiElement>> {
    // Accessibility stubs - returns an empty list for now.
    // Ready for OS Accessibility API native integrations (UIA, AXUIElement, AT-SPI2).
    Ok(Vec::new())
}

pub fn find_button(name: &str) -> Result<Option<UiElement>> {
    let tree = get_ui_tree()?;
    let name_lower = name.to_lowercase();
    Ok(tree.into_iter().find(|el| {
        el.role.to_lowercase() == "button" && el.name.to_lowercase().contains(&name_lower)
    }))
}

pub fn find_input(placeholder: &str) -> Result<Option<UiElement>> {
    let tree = get_ui_tree()?;
    let placeholder_lower = placeholder.to_lowercase();
    Ok(tree.into_iter().find(|el| {
        let role = el.role.to_lowercase();
        (role == "input" || role == "text_input" || role == "edit") && el.name.to_lowercase().contains(&placeholder_lower)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_stubs() {
        let tree = get_ui_tree();
        assert!(tree.is_ok());
        let btn = find_button("Save");
        assert!(btn.is_ok());
        let input = find_input("Username");
        assert!(input.is_ok());
    }
}
