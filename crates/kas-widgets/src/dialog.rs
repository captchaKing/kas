// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Dialog boxes
//!
//! KAS dialog boxes are pre-configured windows, usually allowing some
//! customisation.

use crate::{Label, TextButton};
use kas::event::VirtualKeyCode;
use kas::prelude::*;
use kas::text::format::FormattableText;
use kas::WindowId;

/// A simple message box.
#[derive(Clone, Debug, Widget)]
#[layout(column)]
#[widget(config=noauto)]
pub struct MessageBox<T: FormattableText + 'static> {
    #[widget_core]
    core: CoreData,
    #[layout_data]
    layout_data: <Self as kas::LayoutData>::Data,
    title: String,
    #[widget]
    label: Label<T>,
    #[widget]
    button: TextButton<()>,
}

impl<T: FormattableText + 'static> MessageBox<T> {
    pub fn new<A: ToString>(title: A, message: T) -> Self {
        MessageBox {
            core: Default::default(),
            layout_data: Default::default(),
            title: title.to_string(),
            label: Label::new(message),
            button: TextButton::new_msg("Ok", ()).with_keys(&[
                VirtualKeyCode::Return,
                VirtualKeyCode::Space,
                VirtualKeyCode::NumpadEnter,
            ]),
        }
    }
}

impl<T: FormattableText + 'static> OnMessage<()> for MessageBox<T> {
    fn on_msg(&mut self, mgr: &mut Manager, _: usize, _: ()) -> Response<VoidMsg> {
        mgr.send_action(TkAction::CLOSE);
        Response::None
    }
}

impl<T: FormattableText + 'static> kas::WidgetConfig for MessageBox<T> {
    fn configure(&mut self, mgr: &mut Manager) {
        mgr.enable_alt_bypass(true);
    }
}

impl<T: FormattableText + 'static> kas::Window for MessageBox<T> {
    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> Option<kas::Icon> {
        None // TODO
    }

    fn restrict_dimensions(&self) -> (bool, bool) {
        (true, true)
    }

    // do not support overlays (yet?)
    fn add_popup(&mut self, _: &mut Manager, _: WindowId, _: kas::Popup) {
        panic!("MessageBox does not (currently) support pop-ups");
    }

    fn remove_popup(&mut self, _: &mut Manager, _: WindowId) {}
    fn resize_popups(&mut self, _: &mut Manager) {}
}
