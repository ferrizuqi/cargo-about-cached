mod basic;
mod large;
mod support;

use anyhow::Result;

use basic::BasicScenario;
use large::LargeWorkspaceScenario;
use support::CompatibilityContext;

pub struct CompatibilitySuite {
    context: CompatibilityContext,
}

impl CompatibilitySuite {
    pub fn new() -> Result<Self> {
        Ok(Self {
            context: CompatibilityContext::new()?,
        })
    }

    pub fn run(self) -> Result<()> {
        println!();
        println!("============================================================");
        println!("compatibility: basic");
        println!("============================================================");

        BasicScenario::new(&self.context).run()?;

        println!();
        println!("============================================================");
        println!("compatibility: large workspace");
        println!("============================================================");

        LargeWorkspaceScenario::new(&self.context).run()?;

        println!();
        println!("============================================================");
        println!("compatibility: all scenarios passed");
        println!("============================================================");

        Ok(())
    }
}
