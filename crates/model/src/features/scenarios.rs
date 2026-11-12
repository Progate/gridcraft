//! What-If Analysis ▸ Scenario Manager: named sets of values for a sheet's changing cells
//! (XLSX `<scenarios>`, ECMA-376 Part 1 §18.3.1.76).

use gridcraft_core::{CellRef, RangeRef};
use serde::{Deserialize, Serialize};

/// A sheet's scenarios.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scenarios {
    /// The scenario shown last (`current`), an index into `list`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<u32>,
    /// The scenario selected in the Scenario Manager (`show`), an index into `list`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<u32>,
    /// The cells the scenarios change (`sqref`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sqref: Vec<RangeRef>,
    pub list: Vec<ScenarioDef>,
}

/// One scenario (`<scenario>`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScenarioDef {
    pub name: String,
    /// Prevent changes: on a protected sheet the scenario can't be edited.
    #[serde(default)]
    pub locked: bool,
    /// Hide: on a protected sheet the scenario isn't listed.
    #[serde(default)]
    pub hidden: bool,
    /// Who created or last changed it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub inputs: Vec<ScenarioInput>,
}

/// A changing cell and the value the scenario gives it (`<inputCells>`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScenarioInput {
    pub cell: CellRef,
    /// The value as text (`val`): a number in invariant notation, or the text entered.
    pub value: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub undone: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_fmt_id: Option<u32>,
}
