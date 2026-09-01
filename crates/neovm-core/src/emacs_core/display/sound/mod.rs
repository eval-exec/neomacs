//! Sound playback support, matching GNU Emacs's sound.c.
//!
//! Provides real implementation for:
//! - `play-sound-internal` — audio playback via `rodio` crate (when `sound` feature is enabled)
//!
//! When the `sound` feature is disabled, `play-sound-internal` signals an error
//! matching GNU Emacs behavior when compiled without sound support.

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use strum::{EnumString, IntoStaticStr};

// ---------------------------------------------------------------------------
// GNU Emacs sound spec parsing
// ---------------------------------------------------------------------------
//
// SOUND must be: (sound :file "path" :data "bytes" :volume N :device "dev")
// The leading `sound` symbol is required.
// Either :file or :data must be a string. Volume is 0-100 (int) or 0.0-1.0 (float).
// ---------------------------------------------------------------------------

struct SoundSpec {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    file: Option<String>,
    #[cfg(feature = "sound")]
    data: Option<Vec<u8>>,
    #[cfg(not(feature = "sound"))]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    has_data: bool,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    volume: f32,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    device: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum SoundSpecHead {
    Sound,
}

impl SoundSpecHead {
    fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

fn parse_sound_spec(sound: Value) -> Result<SoundSpec, Flow> {
    let elements = list_to_vec(&sound).unwrap_or_default();
    if elements.is_empty() {
        return Err(signal(
            "error",
            vec![Value::string("Invalid sound specification")],
        ));
    }

    if SoundSpecHead::from_lisp_value(&elements[0]) != Some(SoundSpecHead::Sound) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid sound specification")],
        ));
    }

    if !(elements.len() - 1).is_multiple_of(2) {
        return Err(signal(LispCondition::MalformedKeywordArgList, vec![]));
    }

    let plist_val = if elements.len() > 1 {
        Value::list(elements[1..].to_vec())
    } else {
        Value::NIL
    };

    let file_val = super::plist::plist_get(plist_val, &Value::symbol(":file"));
    let data_val = super::plist::plist_get(plist_val, &Value::symbol(":data"));
    let volume_val = super::plist::plist_get(plist_val, &Value::symbol(":volume"));
    let device_val = super::plist::plist_get(plist_val, &Value::symbol(":device"));

    let file = match file_val {
        Some(v) if !v.is_nil() => match v.kind() {
            ValueKind::String => Some(
                v.as_lisp_string()
                    .unwrap()
                    .as_utf8_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid sound specification")],
                ));
            }
        },
        _ => None,
    };

    #[cfg(feature = "sound")]
    let data = match data_val {
        Some(v) if !v.is_nil() => match v.kind() {
            ValueKind::String => {
                let ls = v.as_lisp_string().unwrap();
                Some(ls.as_bytes().to_vec())
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid sound specification")],
                ));
            }
        },
        _ => None,
    };

    #[cfg(not(feature = "sound"))]
    let has_data = match data_val {
        Some(v) if !v.is_nil() => {
            // Just validate that data was provided (a string).
            match v.kind() {
                ValueKind::String => true,
                _ => {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid sound specification")],
                    ));
                }
            }
        }
        _ => false,
    };

    let no_file = file.is_none();
    #[cfg(feature = "sound")]
    let no_data = data.is_none();
    #[cfg(not(feature = "sound"))]
    let no_data = !has_data;

    if no_file && no_data {
        return Err(signal(
            "error",
            vec![Value::string("Invalid sound specification")],
        ));
    }

    let volume = match volume_val {
        Some(v) if !v.is_nil() => match v.kind() {
            ValueKind::Fixnum(n) => {
                if !(0..=100).contains(&n) {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid sound specification")],
                    ));
                }
                n as f32 / 100.0
            }
            ValueKind::Float => {
                let fv = v.xfloat();
                if !(0.0..=1.0).contains(&fv) {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid sound specification")],
                    ));
                }
                fv as f32
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid sound specification")],
                ));
            }
        },
        _ => 1.0,
    };

    let device = match device_val {
        Some(v) if !v.is_nil() => match v.kind() {
            ValueKind::String => Some(
                v.as_lisp_string()
                    .unwrap()
                    .as_utf8_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid sound specification")],
                ));
            }
        },
        _ => None,
    };

    Ok(SoundSpec {
        file,
        #[cfg(feature = "sound")]
        data,
        #[cfg(not(feature = "sound"))]
        has_data,
        volume,
        device,
    })
}

// ---------------------------------------------------------------------------
// Playback via rodio (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "sound")]
fn open_output_stream(
    device: Option<&str>,
) -> Result<(rodio::OutputStream, rodio::OutputStreamHandle), Flow> {
    if let Some(device_name) = device {
        use rodio::DeviceTrait;
        use rodio::cpal::traits::HostTrait;

        if device_name == "default" {
            return rodio::OutputStream::try_default().map_err(|e| {
                signal(
                    "error",
                    vec![Value::string(&format!("No audio device: {e}"))],
                )
            });
        }

        let host = rodio::cpal::default_host();
        let mut devices = host.output_devices().map_err(|e| {
            signal(
                "error",
                vec![Value::string(&format!("No audio device: {e}"))],
            )
        })?;

        while let Some(output_device) = devices.next() {
            if output_device.name().ok().as_deref() == Some(device_name) {
                return rodio::OutputStream::try_from_device(&output_device).map_err(|e| {
                    signal(
                        "error",
                        vec![Value::string(&format!("No audio device: {e}"))],
                    )
                });
            }
        }

        return Err(signal(
            LispCondition::FileError,
            vec![
                Value::string("Cannot open sound device"),
                Value::string(device_name),
            ],
        ));
    }

    rodio::OutputStream::try_default().map_err(|e| {
        signal(
            "error",
            vec![Value::string(&format!("No audio device: {e}"))],
        )
    })
}

#[cfg(feature = "sound")]
fn play_sound_file(path: &str, volume: f32, device: Option<&str>) -> Result<(), Flow> {
    let file = std::fs::File::open(path).map_err(|e| {
        signal(
            LispCondition::FileError,
            vec![
                Value::string(&format!("Cannot open sound file: {e}")),
                Value::string(path),
            ],
        )
    })?;

    let stream = open_output_stream(device)?;
    let (_stream, stream_handle) = stream;

    let sink = rodio::Sink::try_new(&stream_handle).map_err(|e| {
        signal(
            "error",
            vec![Value::string(&format!("Audio sink error: {e}"))],
        )
    })?;

    sink.set_volume(volume);
    sink.append(
        rodio::Decoder::new(std::io::BufReader::new(file)).map_err(|e| {
            signal(
                "error",
                vec![Value::string(&format!("Cannot decode sound: {e}"))],
            )
        })?,
    );

    sink.sleep_until_end();
    drop(sink);
    Ok(())
}

#[cfg(feature = "sound")]
fn play_sound_data(data: &[u8], volume: f32, device: Option<&str>) -> Result<(), Flow> {
    use std::io::Cursor;

    let stream = open_output_stream(device)?;
    let (_stream, stream_handle) = stream;

    let sink = rodio::Sink::try_new(&stream_handle).map_err(|e| {
        signal(
            "error",
            vec![Value::string(&format!("Audio sink error: {e}"))],
        )
    })?;

    sink.set_volume(volume);
    sink.append(
        rodio::Decoder::new(Cursor::new(data.to_vec())).map_err(|e| {
            signal(
                "error",
                vec![Value::string(&format!("Cannot decode sound: {e}"))],
            )
        })?,
    );

    sink.sleep_until_end();
    drop(sink);
    Ok(())
}

// ---------------------------------------------------------------------------
// Builtin function
// ---------------------------------------------------------------------------

/// (play-sound-internal SOUND)
#[cfg(feature = "sound")]
pub(crate) fn builtin_play_sound_internal(args: Vec<Value>) -> EvalResult {
    super::builtins::expect_args("play-sound-internal", &args, 1)?;

    let spec = parse_sound_spec(args[0])?;

    if let Some(ref path) = spec.file {
        play_sound_file(path, spec.volume, spec.device.as_deref())?;
    } else if let Some(ref data) = spec.data {
        play_sound_data(data, spec.volume, spec.device.as_deref())?;
    }

    Ok(Value::NIL)
}

/// (play-sound-internal SOUND) — stub when sound feature is disabled.
#[cfg(not(feature = "sound"))]
pub(crate) fn builtin_play_sound_internal(args: Vec<Value>) -> EvalResult {
    super::builtins::expect_args("play-sound-internal", &args, 1)?;

    let _spec = parse_sound_spec(args[0])?;

    Err(signal(
        "error",
        vec![Value::string("Sound support not available")],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_spec_head_domain_matches_gnu() {
        crate::test_utils::init_test_tracing();
        assert_eq!(
            SoundSpecHead::from_lisp_value(&Value::symbol("sound")),
            Some(SoundSpecHead::Sound)
        );
        assert_eq!(SoundSpecHead::Sound.name(), "sound");
        assert_eq!(
            SoundSpecHead::from_lisp_value(&Value::symbol("not-sound")),
            None
        );
    }

    #[test]
    fn parse_sound_spec_odd_plist_signals_malformed_keyword_arg_list() {
        crate::test_utils::init_test_tracing();

        let invalid = Value::list(vec![Value::symbol("sound"), Value::symbol(":data")]);
        match parse_sound_spec(invalid) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "malformed-keyword-arg-list");
                assert!(sig.data.is_empty());
            }
            Err(other) => panic!("unexpected flow: {other:?}"),
            Ok(_) => panic!("expected malformed keyword arg list"),
        }
    }

    #[test]
    fn parse_sound_spec_validates_device_like_gnu() {
        crate::test_utils::init_test_tracing();

        let invalid = Value::list(vec![
            Value::symbol("sound"),
            Value::symbol(":data"),
            Value::string(""),
            Value::symbol(":device"),
            Value::fixnum(1),
        ]);
        match parse_sound_spec(invalid) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(sig.data, vec![Value::string("Invalid sound specification")]);
            }
            Err(other) => panic!("unexpected flow: {other:?}"),
            Ok(_) => panic!("expected invalid sound specification"),
        }

        let valid = Value::list(vec![
            Value::symbol("sound"),
            Value::symbol(":data"),
            Value::string(""),
            Value::symbol(":device"),
            Value::string("default"),
        ]);
        let spec = parse_sound_spec(valid).unwrap();
        assert_eq!(spec.device.as_deref(), Some("default"));
    }
}
