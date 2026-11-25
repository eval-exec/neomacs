//! GUI options retained by the existing argv pass for initial font preparation.
//! Originals still reach Lisp at its normal startup phase.

use neomacs_display_protocol::GuiResourceQuery;
use neomacs_display_runtime::gui_resources::GuiResources;

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(super) struct GuiStartupOptions {
    font: Option<String>,
    name: Option<String>,
    resources: Vec<String>,
    inhibit_resources: bool,
}

impl GuiStartupOptions {
    pub(super) fn observe(&mut self, option: &str, value: Option<&str>) {
        match option {
            "-fn" | "-font" => self.font = value.map(str::to_owned),
            "-name" => self.name = value.map(str::to_owned),
            "-xrm" => {
                if let Some(value) = value {
                    self.resources.push(value.to_owned());
                }
            }
            "-Q" | "-quick" | "-no-x-resources" => self.inhibit_resources = true,
            _ => {}
        }
    }

    pub(super) fn prepare(&self, invocation: &str) -> (GuiResources, Option<String>) {
        let mut resources = GuiResources::default();
        resources.set_database(&self.resources.join("\n"));
        let font = self.font.clone().or_else(|| {
            // The terminal layer replaces dots/stars only in invocation-name;
            // an explicit -name reaches x-get-resource's own normalization.
            let instance = self
                .name
                .clone()
                .unwrap_or_else(|| invocation.replace(['.', '*'], "-"));
            resources
                .query(&GuiResourceQuery {
                    name: format!(
                        "{}.font",
                        GuiResourceQuery::normalize_instance_name(&instance)
                    ),
                    class: "Emacs.Font".into(),
                    inhibit_native: self.inhibit_resources,
                })
                .filter(|font| !font.is_empty())
        });
        (resources, font)
    }
}
