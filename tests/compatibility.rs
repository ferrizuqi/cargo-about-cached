mod cargo_about_ref;
mod compatibility_suite;

use anyhow::Result;
use compatibility_suite::CompatibilitySuite;

#[test]
fn output_matches_cargo_about() -> Result<()> {
    CompatibilitySuite::new()?.run()
}
