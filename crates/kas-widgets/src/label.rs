// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text widgets

use kas::text::format::{EditableText, FormattableText};
use kas::theme::TextClass;
use kas::{event, prelude::*};

widget! {
    /// A text label
    ///
    /// This type is generic over the text type. Some aliases are available:
    /// [`StrLabel`], [`StringLabel`], [`AccelLabel`].
    #[derive(Clone, Default, Debug)]
    pub struct Label<T: FormattableText + 'static> {
        #[widget_core]
        core: CoreData,
        label: Text<T>,
    }

    impl Layout for Self {
        #[inline]
        fn size_rules(&mut self, size_mgr: SizeMgr, axis: AxisInfo) -> SizeRules {
            size_mgr.text_bound(&mut self.label, TextClass::Label, axis)
        }

        fn set_rect(&mut self, _: &mut SetRectMgr, rect: Rect, align: AlignHints) {
            self.core.rect = rect;
            self.label.update_env(|env| {
                env.set_bounds(rect.size.into());
                env.set_align(align.unwrap_or(Align::Default, Align::Center));
            });
        }

        #[cfg(feature = "min_spec")]
        default fn draw(&mut self, mut draw: DrawMgr) {
            let mut draw = draw.with_core(self.core_data());
            draw.text_effects(self.core.rect.pos, &self.label, TextClass::Label);
        }
        #[cfg(not(feature = "min_spec"))]
        fn draw(&mut self, mut draw: DrawMgr) {
            let mut draw = draw.with_core(self.core_data());
            draw.text_effects(self.core.rect.pos, &self.label, TextClass::Label);
        }
    }

    impl HasStr for Self {
        fn get_str(&self) -> &str {
            self.label.as_str()
        }
    }

    impl HasString for Self
    where
        T: EditableText,
    {
        fn set_string(&mut self, string: String) -> TkAction {
            kas::text::util::set_string_and_prepare(&mut self.label, string, self.core.rect.size)
        }
    }
}

#[cfg(feature = "min_spec")]
impl Layout for AccelLabel {
    fn draw(&mut self, mut draw: DrawMgr) {
        let mut draw = draw.with_core(self.core_data());
        let accel = draw.ev_state().show_accel_labels();
        draw.text_accel(self.core.rect.pos, &self.label, accel, TextClass::Label);
    }
}

// Str/String representations have no effects, so use simpler draw call
#[cfg(feature = "min_spec")]
impl<'a> Layout for Label<&'a str> {
    fn draw(&mut self, mut draw: DrawMgr) {
        let mut draw = draw.with_core(self.core_data());
        draw.text(self.core.rect.pos, self.label.as_ref(), TextClass::Label);
    }
}
#[cfg(feature = "min_spec")]
impl Layout for StringLabel {
    fn draw(&mut self, mut draw: DrawMgr) {
        let mut draw = draw.with_core(self.core_data());
        draw.text(self.core.rect.pos, self.label.as_ref(), TextClass::Label);
    }
}

/* TODO(specialization): can we support this? min_specialization is not enough.
impl<U, T: From<U> + FormattableText + 'static> From<U> for Label<T> {
    default fn from(text: U) -> Self {
        let text = T::from(text);
        Label::new(text)
    }
}*/

impl<T: FormattableText + 'static> From<T> for Label<T> {
    fn from(label: T) -> Self {
        Label::new(label)
    }
}

impl<'a> From<&'a str> for Label<String> {
    fn from(label: &'a str) -> Self {
        Label::new(label.to_string())
    }
}

impl<T: FormattableText + 'static> Label<T> {
    /// Construct from `label`
    #[inline]
    pub fn new(label: T) -> Self {
        Label {
            core: Default::default(),
            label: Text::new_multi(label),
        }
    }

    /// Set text in an existing `Label`
    ///
    /// Note: this must not be called before fonts have been initialised
    /// (usually done by the theme when the main loop starts).
    pub fn set_text(&mut self, text: T) -> TkAction {
        kas::text::util::set_text_and_prepare(&mut self.label, text, self.core.rect.size)
    }
}

/// Label with `&'static str` as backing type
///
/// Warning: this type does not support [`HasString`]. Assignment is possible
/// via [`Label::set_text`], but only for `&'static str`, so most of the time
/// [`StringLabel`] will be preferred when assignment is required.
/// (Also note that the overhead of allocating and copying a `String` is
/// irrelevant considering those used for text layout and drawing.)
pub type StrLabel = Label<&'static str>;

/// Label with `String` as backing type
pub type StringLabel = Label<String>;

/// A label supporting an accelerator key
///
/// Accelerator keys are not useful on plain labels. To be useful, a parent
/// widget must do something like:
/// ```no_test
/// impl WidgetConfig for Self {
///     fn configure(&mut self, mgr: &mut EventMgr) {
///         let target = self.id(); // widget receiving Event::Activate
///         mgr.add_accel_keys(target, self.label.keys());
///     }
//// }
/// ```
pub type AccelLabel = Label<AccelString>;

impl AccelLabel {
    /// Get the accelerator keys
    pub fn keys(&self) -> &[event::VirtualKeyCode] {
        self.label.text().keys()
    }
}

impl SetAccel for AccelLabel {
    fn set_accel_string(&mut self, string: AccelString) -> TkAction {
        let mut action = TkAction::empty();
        if self.label.text().keys() != string.keys() {
            action |= TkAction::RECONFIGURE;
        }
        action | kas::text::util::set_text_and_prepare(&mut self.label, string, self.core.rect.size)
    }
}
