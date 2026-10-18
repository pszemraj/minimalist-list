//! Desktop cursor settings consumed before winit creates its X11 cursor cache.
//!
//! XSETTINGS format: https://specifications.freedesktop.org/xsettings/0.5/
//! GTK maps Gtk/CursorThemeName, Gtk/CursorThemeSize and Gdk/WindowScalingFactor,
//! then requests cursor pixels as logical theme size times the display scale.

#[derive(Default, Debug, PartialEq, Eq)]
/// Cursor preferences advertised by the desktop's XSETTINGS owner.
pub(crate) struct CursorPreferences {
    theme: Option<String>,
    size: Option<i32>,
    scale: Option<i32>,
}

impl CursorPreferences {
    /// Resolves per-display overrides without replacing explicit Xcursor environment choices.
    ///
    /// # Arguments
    /// - `explicit_theme` - Whether XCURSOR_THEME was provided by the user.
    /// - `explicit_size` - Whether XCURSOR_SIZE was provided by the user.
    ///
    /// # Returns
    /// The optional desktop theme name and physical cursor size.
    pub(crate) fn overrides(
        &self,
        explicit_theme: bool,
        explicit_size: bool,
    ) -> (Option<&str>, Option<i32>) {
        let theme = (!explicit_theme).then_some(self.theme.as_deref()).flatten();
        let size = if explicit_size {
            None
        } else {
            self.size
                .and_then(|size| size.checked_mul(self.scale.unwrap_or(1)))
        };
        (theme, size)
    }
}

/// Parses bounded XSETTINGS data and ignores unrelated desktop settings.
///
/// # Returns
/// Cursor preferences, or `None` for malformed or unsupported property data.
pub(crate) fn parse_xsettings(data: &[u8]) -> Option<CursorPreferences> {
    let mut reader = Reader {
        data,
        position: 0,
        little: match data.first()? {
            0 | b'l' => true,
            1 | b'B' => false,
            _ => return None,
        },
    };
    reader.take(8)?;
    let count = reader.u32()?;
    let mut preferences = CursorPreferences::default();
    for _ in 0..count {
        let kind = *reader.take(1)?.first()?;
        reader.take(1)?;
        let name_length = usize::from(reader.u16()?);
        let name = reader.take(name_length)?;
        reader.pad()?;
        reader.take(4)?; // Last-change serial.
        match kind {
            0 => {
                let integer = reader.i32()?;
                if integer > 0 {
                    if name == b"Gtk/CursorThemeSize" {
                        preferences.size = Some(integer);
                    }
                    if name == b"Gdk/WindowScalingFactor" {
                        preferences.scale = Some(integer);
                    }
                }
            }
            1 => {
                let length = usize::try_from(reader.u32()?).ok()?;
                let string = reader.take(length)?;
                reader.pad()?;
                if name == b"Gtk/CursorThemeName" {
                    let theme = std::str::from_utf8(string).ok()?;
                    if !theme.is_empty() && !theme.contains('\0') {
                        preferences.theme = Some(theme.to_owned());
                    }
                }
            }
            2 => {
                reader.take(8)?;
            }
            _ => return None,
        }
    }
    Some(preferences)
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
    little: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(length)?;
        let bytes = self.data.get(self.position..end)?;
        self.position = end;
        Some(bytes)
    }

    fn pad(&mut self) -> Option<()> {
        self.take((4 - self.position % 4) % 4)?;
        Some(())
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.take(2)?.try_into().ok()?;
        Some(if self.little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes = self.take(4)?.try_into().ok()?;
        Some(if self.little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    fn i32(&mut self) -> Option<i32> {
        let bytes = self.take(4)?.try_into().ok()?;
        Some(if self.little {
            i32::from_le_bytes(bytes)
        } else {
            i32::from_be_bytes(bytes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_wire_property_uses_numeric_lsbfirst_with_32_bit_padded_records() {
        // GNOME's xsettings-manager writes xsettings_byte_order() (LSBFirst=0),
        // CARD32 count, then a type/name/serial/value record. This literal is
        // independent of the fixture serializer below.
        let property =
            b"\x00\0\0\0\0\0\0\0\x01\0\0\0\x00\0\x13\0Gtk/CursorThemeSize\0\0\0\0\0\x18\0\0\0";
        assert_eq!(
            parse_xsettings(property).unwrap().overrides(false, false),
            (None, Some(24))
        );
    }

    fn fixture(little: bool, theme: &str, size: i32, scale: i32) -> Vec<u8> {
        let mut data = vec![if little { 0 } else { 1 }, 0, 0, 0, 0, 0, 0, 0];
        let word = |value: u32| {
            if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            }
        };
        data.extend(word(3));
        for (name, value) in [
            ("Gtk/CursorThemeName", None),
            ("Gtk/CursorThemeSize", Some(size)),
            ("Gdk/WindowScalingFactor", Some(scale)),
        ] {
            data.extend([u8::from(value.is_none()), 0]);
            let length = name.len() as u16;
            data.extend(if little {
                length.to_le_bytes()
            } else {
                length.to_be_bytes()
            });
            data.extend(name.as_bytes());
            while data.len() % 4 != 0 {
                data.push(0);
            }
            data.extend(word(0));
            if let Some(value) = value {
                data.extend(word(value as u32));
            } else {
                data.extend(word(theme.len() as u32));
                data.extend(theme.as_bytes());
                while data.len() % 4 != 0 {
                    data.push(0);
                }
            }
        }
        data
    }

    #[test]
    fn desktop_theme_and_size_follow_gtk_scale_in_either_byte_order() {
        for little in [false, true] {
            let mut data = fixture(little, "Adwaita", 24, 2);
            let preferences = parse_xsettings(&data).unwrap();
            assert_eq!(
                preferences.overrides(false, false),
                (Some("Adwaita"), Some(48))
            );
            data[0] = if little { b'l' } else { b'B' };
            assert_eq!(parse_xsettings(&data), Some(preferences));
        }
    }

    #[test]
    fn explicit_environment_choices_win_independently() {
        let preferences = parse_xsettings(&fixture(true, "Breeze", 32, 2)).unwrap();
        assert_eq!(preferences.overrides(true, false), (None, Some(64)));
        assert_eq!(preferences.overrides(false, true), (Some("Breeze"), None));
        assert_eq!(preferences.overrides(true, true), (None, None));
    }

    #[test]
    fn invalid_cursor_values_are_ignored_and_truncated_properties_are_rejected() {
        let data = fixture(true, "Breeze", 32, 2);
        for length in 0..data.len() {
            assert!(parse_xsettings(&data[..length]).is_none());
        }
        let invalid = parse_xsettings(&fixture(true, "bad\0theme", -1, 0)).unwrap();
        assert_eq!(invalid.overrides(false, false), (None, None));
        let invalid_scale = parse_xsettings(&fixture(true, "Breeze", 32, -1)).unwrap();
        assert_eq!(
            invalid_scale.overrides(false, false),
            (Some("Breeze"), Some(32))
        );
        let large = parse_xsettings(&fixture(true, "Breeze", i32::MAX, 2)).unwrap();
        assert_eq!(large.overrides(false, false), (Some("Breeze"), None));
    }
}
