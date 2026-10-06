use std::{collections::BTreeMap, fs, path::Path};

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

pub fn generate_output<'a>(
    licenses: &'a [KrateLicense<'a>],
    config: Config,
    template_path: &Path,
    output_path: &Path,
) -> Result<()> {
    anyhow::ensure!(
        template_path.exists(),
        "template does not exist: {}",
        template_path.display()
    );

    anyhow::ensure!(
        template_path.is_file(),
        "template must be a file: {}",
        template_path.display()
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

    register_helpers(&mut handlebars);

    handlebars
        .register_template_file("template", template_path)
        .with_context(|| format!("failed to load template: {}", template_path.display()))?;

    let output = handlebars
        .render("template", &license_list)
        .context("failed to render template")?;

    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory: {}", parent.display()))?;
    }

    fs::write(output_path, output)
        .with_context(|| format!("failed to write output: {}", output_path.display()))?;

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
