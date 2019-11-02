// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Widget classifications
//!
//! Each widget must have one of the classifications defined in the [`Class`]
//! enumeration. In some of these cases, the widget must implement additional
//! functionality (usually on itself).

use crate::traits::{Editable, HasBoolText, HasText};
use std::fmt;

/// Alignment of contents
pub enum Align {
    /// Align to top or left (for left-to-right text)
    Begin,
    /// Align to centre
    Center,
    /// Align to bottom or right (for left-to-right text)
    End,
    /// Attempt to align to both margins, padding with space
    Justify,
}

/// Widget classifications
pub enum Class<'a> {
    Container,
    // Dialog,
    Label(&'a dyn HasText),
    Entry(&'a dyn Editable),
    Button(&'a dyn HasText),
    CheckBox(&'a dyn HasBoolText),
    Frame,
    Window,
}

impl<'a> fmt::Debug for Class<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Class::{}",
            match self {
                Class::Container => "Container",
                Class::Label(_) => "Label",
                Class::Entry(_) => "Entry",
                Class::Button(_) => "Button",
                Class::CheckBox(_) => "CheckBox",
                Class::Frame => "Frame",
                Class::Window => "Window",
            }
        )
    }
}

impl<'a> Class<'a> {
    /// Get widget text, if any
    pub fn text(&'a self) -> Option<&'a str> {
        match self {
            Class::Container => None,
            Class::Label(cls) => Some(cls.get_text()),
            Class::Entry(cls) => Some(cls.get_text()),
            Class::Button(cls) => Some(cls.get_text()),
            Class::CheckBox(cls) => Some(cls.get_text()),
            Class::Frame => None,
            Class::Window => None,
        }
    }

    /// Get widget alignment for horizontal and vertical axes respectively
    // TODO: allow customisation?
    pub fn alignments(&'a self) -> (Align, Align) {
        match self {
            Class::Container | Class::Frame | Class::Window => (Align::Justify, Align::Justify),
            Class::Label(_) => (Align::Begin, Align::Center),
            Class::Entry(_) => (Align::Begin, Align::Begin),
            Class::Button(_) | Class::CheckBox(_) => (Align::Center, Align::Center),
        }
    }
}
