use serde::{Deserialize, Serialize};

/// Tab bar position in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabBarPosition {
    Top,
    Bottom,
}
