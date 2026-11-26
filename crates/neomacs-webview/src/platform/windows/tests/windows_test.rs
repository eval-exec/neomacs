use super::ApartmentScoped;
use std::cell::RefCell;
use std::rc::Rc;

struct DropRecorder {
    label: &'static str,
    log: Rc<RefCell<Vec<&'static str>>>,
}

impl Drop for DropRecorder {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.label);
    }
}

struct FakePlatformResources {
    _completions: DropRecorder,
    _hosts: DropRecorder,
    _composition_device: DropRecorder,
    _environment: DropRecorder,
}

#[test]
fn apartment_scoped_drops_com_resources_before_apartment_on_drop_and_unwind() {
    let log = Rc::new(RefCell::new(Vec::new()));
    {
        let _scoped = ApartmentScoped::new(
            FakePlatformResources {
                _completions: DropRecorder {
                    label: "completions",
                    log: Rc::clone(&log),
                },
                _hosts: DropRecorder {
                    label: "hosts",
                    log: Rc::clone(&log),
                },
                _composition_device: DropRecorder {
                    label: "composition_device",
                    log: Rc::clone(&log),
                },
                _environment: DropRecorder {
                    label: "environment",
                    log: Rc::clone(&log),
                },
            },
            Some(DropRecorder {
                label: "apartment",
                log: Rc::clone(&log),
            }),
        );
    }
    assert_eq!(
        *log.borrow(),
        vec![
            "completions",
            "hosts",
            "composition_device",
            "environment",
            "apartment",
        ]
    );

    log.borrow_mut().clear();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scoped = ApartmentScoped::new(
            FakePlatformResources {
                _completions: DropRecorder {
                    label: "completions",
                    log: Rc::clone(&log),
                },
                _hosts: DropRecorder {
                    label: "hosts",
                    log: Rc::clone(&log),
                },
                _composition_device: DropRecorder {
                    label: "composition_device",
                    log: Rc::clone(&log),
                },
                _environment: DropRecorder {
                    label: "environment",
                    log: Rc::clone(&log),
                },
            },
            Some(DropRecorder {
                label: "apartment",
                log: Rc::clone(&log),
            }),
        );
        panic!("simulated unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(
        *log.borrow(),
        vec![
            "completions",
            "hosts",
            "composition_device",
            "environment",
            "apartment",
        ]
    );
}
