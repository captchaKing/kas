// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Markdown parsing demo

use kas::class::HasStr;
use kas::event::{EventMgr, VoidMsg};
use kas::macros::make_widget;
use kas::text::format::Markdown;
use kas::widgets::{EditBox, Label, ScrollBarRegion, TextButton, Window};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let theme = kas::theme::FlatTheme::new();
    let shell = kas::shell::Toolkit::new(theme)?;

    let doc = r"Markdown document
================

Markdown supports *italic* and **bold** highlighting, ***both***, even with*in* w**o**rds.
As an extension, it also supports ~~strikethrough~~.

Inline `code = 2;` is supported. Code blocks are supported:
```
let x = 1;
let y = x + 1;
```

Markdown supports explicit line breaks —  
via two trailing spaces.  
It also supports lists:

1.  First item
2.  Second item

-   Unenumerated item
-   Another item
";

    let window = Window::new(
        "Markdown parser",
        make_widget! {
            #[widget{
                layout = grid: {
                    0..2, 0: self.editor;
                    0, 1: self.label;
                    1, 1: self.b_update;
                };
            }]
            #[handler(msg = VoidMsg)]
            struct {
                #[widget] editor: EditBox =
                    EditBox::new(doc).multi_line(true),
                #[widget] label: ScrollBarRegion<Label<Markdown>> =
                    ScrollBarRegion::new(Label::new(Markdown::new(doc)?)),
                #[widget(use_msg=update)] b_update = TextButton::new_msg("&Update", ()),
            }
            impl Self {
                fn update(&mut self, mgr: &mut EventMgr, _: ()) {
                    let text = match Markdown::new(self.editor.get_str()) {
                        Ok(text) => text,
                        Err(err) => {
                            let string = format!("```\n{}\n```", err);
                            Markdown::new(&string).unwrap()
                        }
                    };
                    // TODO: this should update the size requirements of the inner area
                    *mgr |= self.label.set_text(text);
                }
            }
        },
    );

    shell.with(window)?.run()
}
