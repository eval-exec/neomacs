//! Native database contracts run in nextest's isolated test processes.

#[cfg(windows)]
#[test]
fn windows_resources_preserve_gnu_precedence_types_and_empty_values() {
    use neomacs_display_runtime::gui_resources::GuiResources;
    use neovm_core::emacs_core::display_host::GuiResourceQuery;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RegOverridePredefKey,
    };
    use winreg::{
        HKCU, RegKey, RegValue,
        enums::{REG_EXPAND_SZ, REG_SZ},
        types::ToRegValue,
    };

    struct IsolatedHives {
        path: String,
        _user: RegKey,
        _machine: RegKey,
    }
    impl Drop for IsolatedHives {
        fn drop(&mut self) {
            // SAFETY: this isolated test process owns both overrides. Restore
            // the predefined roots before removing only its private fixture.
            unsafe {
                RegOverridePredefKey(HKEY_CURRENT_USER, std::ptr::null_mut());
                RegOverridePredefKey(HKEY_LOCAL_MACHINE, std::ptr::null_mut());
            }
            HKCU.delete_subkey_all(&self.path).unwrap();
        }
    }
    let path = format!(r"Software\Neomacs\ResourceContract-{}", std::process::id());
    assert!(HKCU.open_subkey(&path).is_err(), "fixture key must be new");
    let (root, _) = HKCU.create_subkey(&path).unwrap();
    let (user, _) = root.create_subkey("user").unwrap();
    let (machine, _) = root.create_subkey("machine").unwrap();
    let (user_values, _) = user.create_subkey(r"SOFTWARE\GNU\Emacs").unwrap();
    let (machine_values, _) = machine.create_subkey(r"SOFTWARE\GNU\Emacs").unwrap();
    let fixture = IsolatedHives {
        path,
        _user: user,
        _machine: machine,
    };
    // SAFETY: handles remain owned by fixture until after overrides are cleared.
    unsafe {
        assert_eq!(
            RegOverridePredefKey(HKEY_CURRENT_USER, fixture._user.raw_handle()),
            0
        );
        assert_eq!(
            RegOverridePredefKey(HKEY_LOCAL_MACHINE, fixture._machine.raw_handle()),
            0
        );
    }
    let mut resources = GuiResources::default();
    let mut query = GuiResourceQuery {
        name: "editor.font".into(),
        class: "Emacs.Font".into(),
        inhibit_native: false,
    };
    machine_values
        .set_value(&query.class, &"machine class")
        .unwrap();
    machine_values
        .set_value(&query.name, &"machine instance")
        .unwrap();
    assert_eq!(resources.query(&query).as_deref(), Some("machine instance"));
    user_values.set_value(&query.class, &"user class").unwrap();
    assert_eq!(resources.query(&query).as_deref(), Some("user class"));
    user_values
        .set_value(&query.name, &"user instance")
        .unwrap();
    assert_eq!(resources.query(&query).as_deref(), Some("user instance"));
    user_values.set_value(&query.name, &"").unwrap();
    assert_eq!(resources.query(&query).as_deref(), Some(""));
    let mut expanded = "%WINDIR%".to_reg_value();
    expanded.vtype = REG_EXPAND_SZ;
    user_values.set_raw_value(&query.name, &expanded).unwrap();
    assert_eq!(resources.query(&query).as_deref(), Some("user class"));
    let long = "字Mono".repeat(2048);
    user_values.set_value(&query.name, &long).unwrap();
    assert_eq!(resources.query(&query), Some(long));
    user_values
        .set_raw_value(
            &query.name,
            &RegValue {
                vtype: REG_SZ,
                bytes: vec![b'A', 0].into(),
            },
        )
        .unwrap();
    assert_eq!(resources.query(&query).as_deref(), Some("A"));
    query.inhibit_native = true;
    assert_eq!(resources.query(&query), None);
    resources.set_database(
        "Emacs.Font: explicit class\n Editor.Font : explicit instance  \nEditor.Font: duplicate",
    );
    assert_eq!(
        resources.query(&query).as_deref(),
        Some("explicit instance  ")
    );
    resources.set_database("Editor.Font:\nEmacs.Font: fallback");
    assert_eq!(resources.query(&query).as_deref(), Some(""));
}

#[cfg(target_os = "macos")]
#[test]
fn cocoa_resources_use_standard_defaults_class_and_gnu_boolean_prefixes() {
    use neomacs_display_runtime::gui_resources::GuiResources;
    use neovm_core::emacs_core::display_host::GuiResourceQuery;
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSDictionary, NSString, NSUserDefaults};

    autoreleasepool(|_| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let domain = NSString::from_str("NeomacsResourceContract");
        let key = NSString::from_str("ResourceContract");
        // A volatile suite is process-local and never changes desktop defaults.
        defaults.addSuiteNamed(&domain);
        struct Suite<'a>(&'a NSUserDefaults, &'a NSString);
        impl Drop for Suite<'_> {
            fn drop(&mut self) {
                self.0.removeSuiteNamed(self.1);
                self.0.removeVolatileDomainForName(self.1);
            }
        }
        let _suite = Suite(&defaults, &domain);
        let resources = GuiResources::default();
        let mut query = GuiResourceQuery {
            name: "ignored.font".into(),
            class: "Emacs.ResourceContract".into(),
            inhibit_native: false,
        };
        for (value, expected) in [
            ("Mono-13", "Mono-13"),
            ("YESplease", "true"),
            ("Nothing", "false"),
            ("", ""),
        ] {
            let value = NSString::from_str(value);
            let dictionary = NSDictionary::<NSString, objc2::runtime::AnyObject>::from_slices(
                &[&*key],
                &[&*value],
            );
            // SAFETY: the dictionary contains only NSString keys and values.
            unsafe {
                defaults.setVolatileDomain_forName(&dictionary, &domain);
            }
            assert_eq!(resources.query(&query).as_deref(), Some(expected));
        }
        query.inhibit_native = true;
        assert_eq!(resources.query(&query), None);
    });
}
