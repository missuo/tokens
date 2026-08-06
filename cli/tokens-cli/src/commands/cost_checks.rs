//! Shared finite cost arithmetic used by v3 report and snapshot builders.

use anyhow::{bail, Result};

const COST_TOLERANCE: f64 = 1e-9;

pub(crate) fn checked_cost_sum(left: f64, right: f64, label: &str) -> Result<f64> {
    if !left.is_finite() || !right.is_finite() {
        bail!("{label} cost operands must be finite");
    }
    let total = left + right;
    if !total.is_finite() {
        bail!("{label} cost accumulation overflow");
    }
    Ok(total)
}

pub(crate) fn cost_matches(left: f64, right: f64) -> bool {
    if !left.is_finite() || !right.is_finite() || !COST_TOLERANCE.is_finite() {
        return false;
    }
    let difference = (left - right).abs();
    let tolerance = COST_TOLERANCE * left.abs().max(right.abs()).max(1.0);
    difference.is_finite() && tolerance.is_finite() && difference <= tolerance
}
