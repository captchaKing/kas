// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Cursor gallery

use kas::event::CursorIcon;
use kas::prelude::*;
use kas::widget::{Column, Label, StrLabel, Window};

#[derive(Clone, Debug, Widget)]
#[widget(config = noauto)]
#[layout(single)]
struct CursorWidget {
    #[widget_core]
    core: CoreData,
    #[widget]
    label: StrLabel,
    cursor: CursorIcon,
}
impl WidgetConfig for CursorWidget {
    fn cursor_icon(&self) -> CursorIcon {
        self.cursor
    }
}

// Using a macro lets us stringify! the type name
macro_rules! cursor {
    ($name: tt) => {
        CursorWidget {
            core: Default::default(),
            label: Label::new(stringify!($name)),
            cursor: CursorIcon::$name,
        }
    };
}

fn main() -> Result<(), kas_wgpu::Error> {
    env_logger::init();

    // These are winit::window::CursorIcon enum variants
    let column = Column::new(vec![
        cursor!(Default),
        cursor!(Crosshair),
        cursor!(Hand),
        cursor!(Arrow),
        cursor!(Move),
        cursor!(Text),
        cursor!(Wait),
        cursor!(Help),
        cursor!(Progress),
        cursor!(NotAllowed),
        cursor!(ContextMenu),
        cursor!(Cell),
        cursor!(VerticalText),
        cursor!(Alias),
        cursor!(Copy),
        cursor!(NoDrop),
        cursor!(Grab),
        cursor!(Grabbing),
        cursor!(AllScroll),
        cursor!(ZoomIn),
        cursor!(ZoomOut),
        cursor!(EResize),
        cursor!(NResize),
        cursor!(NeResize),
        cursor!(NwResize),
        cursor!(SResize),
        cursor!(SeResize),
        cursor!(SwResize),
        cursor!(WResize),
        cursor!(EwResize),
        cursor!(NsResize),
        cursor!(NeswResize),
        cursor!(NwseResize),
        cursor!(ColResize),
        cursor!(RowResize),
    ]);

    let gallery = column.map_msg(|_, (_, msg)| msg);
    let window = Window::new("Cursor gallery", gallery);
    let theme = kas_theme::FlatTheme::new();
    kas_wgpu::Toolkit::new(theme)?.with(window)?.run()
}
