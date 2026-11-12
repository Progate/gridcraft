//! Background error checking: the rules Excel's File › Options › Formulas › Error checking rules
//! lists, and the errors a sheet ignores (`<ignoredErrors>`).

use gridcraft_core::RangeRef;
use serde::{Deserialize, Serialize};

/// One error checking rule, as `<ignoredError>` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorRule {
    /// Cells containing formulas that result in an error.
    EvalError,
    /// Inconsistent calculated column formula in tables.
    CalculatedColumn,
    /// Cells containing years represented as 2 digits.
    TwoDigitTextYear,
    /// Numbers formatted as text or preceded by an apostrophe.
    NumberStoredAsText,
    /// Formulas inconsistent with other formulas in the region.
    Formula,
    /// Formulas which omit cells in a region.
    FormulaRange,
    /// Unlocked cells containing formulas.
    UnlockedFormula,
    /// Formulas referring to empty cells.
    EmptyCellReference,
    /// Data entered in a table is invalid.
    ListDataValidation,
}

impl ErrorRule {
    /// Every rule, in the order Excel's Options page lists them.
    pub const ALL: [ErrorRule; 9] = [
        ErrorRule::EvalError,
        ErrorRule::CalculatedColumn,
        ErrorRule::TwoDigitTextYear,
        ErrorRule::NumberStoredAsText,
        ErrorRule::Formula,
        ErrorRule::FormulaRange,
        ErrorRule::UnlockedFormula,
        ErrorRule::EmptyCellReference,
        ErrorRule::ListDataValidation,
    ];

    /// The `<ignoredError>` attribute (CT_IgnoredError), also the rule's id in commands.
    pub fn attr(self) -> &'static str {
        match self {
            ErrorRule::EvalError => "evalError",
            ErrorRule::CalculatedColumn => "calculatedColumn",
            ErrorRule::TwoDigitTextYear => "twoDigitTextYear",
            ErrorRule::NumberStoredAsText => "numberStoredAsText",
            ErrorRule::Formula => "formula",
            ErrorRule::FormulaRange => "formulaRange",
            ErrorRule::UnlockedFormula => "unlockedFormula",
            ErrorRule::EmptyCellReference => "emptyCellReference",
            ErrorRule::ListDataValidation => "listDataValidation",
        }
    }

    pub fn from_attr(a: &str) -> Option<ErrorRule> {
        ErrorRule::ALL.into_iter().find(|r| r.attr() == a)
    }

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// A set of rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorRules(pub u16);

impl ErrorRules {
    pub const NONE: ErrorRules = ErrorRules(0);

    /// The rules Excel checks out of the box: all but Formulas referring to empty cells.
    pub fn excel_default() -> ErrorRules {
        let mut r = ErrorRules::NONE;
        for rule in ErrorRule::ALL {
            if rule != ErrorRule::EmptyCellReference {
                r.insert(rule);
            }
        }
        r
    }
    pub fn contains(self, r: ErrorRule) -> bool {
        self.0 & r.bit() != 0
    }
    pub fn insert(&mut self, r: ErrorRule) {
        self.0 |= r.bit();
    }
    pub fn set(&mut self, r: ErrorRule, on: bool) {
        if on {
            self.0 |= r.bit();
        } else {
            self.0 &= !r.bit();
        }
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn iter(self) -> impl Iterator<Item = ErrorRule> {
        ErrorRule::ALL.into_iter().filter(move |r| self.contains(*r))
    }
}

/// One `<ignoredError>`: rules not reported for the cells of `ranges` (Ignore Error).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredError {
    pub ranges: Vec<RangeRef>,
    pub rules: ErrorRules,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_round_trip_through_their_attributes() {
        for r in ErrorRule::ALL {
            assert_eq!(ErrorRule::from_attr(r.attr()), Some(r));
        }
        let d = ErrorRules::excel_default();
        assert!(d.contains(ErrorRule::EvalError) && !d.contains(ErrorRule::EmptyCellReference));
        assert_eq!(d.iter().count(), 8);
    }
}
