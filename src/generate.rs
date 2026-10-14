use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use cargo_about::{
    generate::generate as build_license_list,
    licenses::{KrateLicense, config::Config, resolution},
};
use codespan_reporting::term::{
    self,
    termcolor::{ColorChoice, StandardStream},
};
use handlebars::{
    Context as HandlebarsContext, Handlebars, Helper, HelperResult, Output, RenderContext,
    RenderErrorReason,
};

#[derive(Debug)]
pub struct OutputGenerator {
    template: PathBuf,
    output: PathBuf,
}

impl OutputGenerator {
    pub fn new(template: PathBuf, output: PathBuf) -> Self {
        Self { template, output }
    }

    pub fn output_path(&self) -> &Path {
        &self.output
    }

    pub fn generate<'a>(&self, licenses: &'a [KrateLicense<'a>], config: Config) -> Result<()> {
        anyhow::ensure!(
            self.template.exists(),
            "template does not exist: {}",
            self.template.display()
        );

        anyhow::ensure!(
            self.template.is_file(),
            "template must be a file: {}",
            self.template.display()
        );

        let Config {
            accepted, crates, ..
        } = config;

        let crate_configs = crates
            .into_iter()
            .map(|(name, config)| (name, config.value))
            .collect::<BTreeMap<_, _>>();

        let mut files = resolution::Files::new();
        let resolved = resolution::resolve(licenses, &accepted, &crate_configs, &mut files, false);

        let stream = StandardStream::stderr(ColorChoice::Auto);
        let diagnostic_config = term::Config::default();

        let license_list = build_license_list(licenses, &resolved, |diagnostics| {
            let mut stream = stream.lock();

            for diagnostic in diagnostics {
                let _ = term::emit_to_io_write(&mut stream, &diagnostic_config, &files, diagnostic);
            }
        })
        .context("failed to generate license list")?;

        let mut handlebars = Handlebars::new();

        Self::register_helpers(&mut handlebars);

        handlebars
            .register_template_file("template", &self.template)
            .with_context(|| format!("failed to load template: {}", self.template.display()))?;

        let output = handlebars
            .render("template", &license_list)
            .context("failed to render template")?;

        if let Some(parent) = self
            .output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory: {}", parent.display())
            })?;
        }

        std::fs::write(&self.output, output)
            .with_context(|| format!("failed to write output: {}", self.output.display()))?;

        Ok(())
    }

    fn register_helpers(handlebars: &mut Handlebars<'_>) {
        handlebars.register_helper(
            "json",
            Box::new(
                |helper: &Helper<'_>,
                 _registry: &Handlebars<'_>,
                 _context: &HandlebarsContext,
                 _render_context: &mut RenderContext<'_, '_>,
                 output: &mut dyn Output|
                 -> HelperResult {
                    let parameter = helper
                        .param(0)
                        .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("json", 0))?;

                    match serde_json::to_string_pretty(parameter.value()) {
                        Ok(json) => {
                            output.write(&json)?;
                            Ok(())
                        }
                        Err(error) => Err(RenderErrorReason::Other(error.to_string()).into()),
                    }
                },
            ),
        );
    }
}
