//! GNU platform resource databases. Native handles remain inside the adapter;
//! empty strings remain present until the Lisp boundary decides their meaning.

use neovm_core::emacs_core::display_host::GuiResourceQuery;

#[cfg(target_os = "macos")]
mod cocoa;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Default)]
pub struct GuiResources {
    #[cfg(target_os = "windows")]
    explicit: Vec<(String, String)>,
}

impl GuiResources {
    #[cfg(target_os = "macos")]
    pub fn ns_resource(&self, name: &str) -> Option<String> {
        cocoa::get(name)
    }

    #[cfg(target_os = "macos")]
    pub fn set_ns_resource(&mut self, name: &str, value: Option<&str>) {
        cocoa::set(name, value);
    }

    pub fn set_database(&mut self, resources: &str) {
        #[cfg(target_os = "windows")]
        {
            // GNU w32_term_init strips ASCII spaces from options and before
            // values. Trailing value whitespace and first-match order survive.
            self.explicit = resources
                .split('\0')
                .next()
                .unwrap_or_default()
                .split('\n')
                .take_while(|line| line.chars().any(|ch| ch != ' '))
                .filter_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    Some((
                        key.replace(' ', ""),
                        value.trim_start_matches(' ').to_owned(),
                    ))
                })
                .collect();
        }
        #[cfg(not(target_os = "windows"))]
        let _ = resources;
    }

    pub fn query(&self, query: &GuiResourceQuery) -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            for key in [&query.name, &query.class] {
                if let Some((_, value)) = self
                    .explicit
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(key))
                {
                    return Some(value.clone());
                }
            }
            if query.inhibit_native {
                return None;
            }
            windows::query(query)
        }
        #[cfg(target_os = "macos")]
        {
            if query.inhibit_native {
                return None;
            }
            cocoa::get(query.class.strip_prefix("Emacs.").unwrap_or(&query.class)).map(|value| {
                if value
                    .get(..3)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("YES"))
                {
                    "true".into()
                } else if value
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("NO"))
                {
                    "false".into()
                } else {
                    value
                }
            })
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let _ = query;
            None
        }
    }
}
