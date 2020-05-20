// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Counter example (simple button)
#![feature(proc_macro_hygiene)]

use std::time::{Duration, Instant};

use kas::class::HasText;
use kas::event::{Event, Handler, Manager, Response, VoidMsg};
use kas::macros::{make_widget, VoidMsg};
use kas::widget::{Frame, Label, TextButton, Window};
use kas::{ThemeApi, WidgetCore};

#[derive(Clone, Debug, VoidMsg)]
enum Control {
    Reset,
    Start,
}

// Unlike most examples, we encapsulate the GUI configuration into a function.
// There's no reason for this, but it demonstrates usage of Toolkit::add_boxed
fn make_window() -> Box<dyn kas::Window> {
    let stopwatch = make_widget! {
        #[layout(row)]
        #[widget(config=noauto)]
        struct {
            #[widget] display: impl HasText = Frame::new(Label::new("0.000")),
            #[widget(handler = handle_button)] b_reset = TextButton::new("&reset", Control::Reset),
            #[widget(handler = handle_button)] b_start = TextButton::new("&start / &stop", Control::Start),
            saved: Duration = Duration::default(),
            start: Option<Instant> = None,
        }
        impl {
            fn handle_button(&mut self, mgr: &mut Manager, msg: Control) -> Response<VoidMsg> {
                match msg {
                    Control::Reset => {
                        self.saved = Duration::default();
                        self.start = None;
                        *mgr += self.display.set_text("0.000");
                    }
                    Control::Start => {
                        if let Some(start) = self.start {
                            self.saved += Instant::now() - start;
                            self.start = None;
                        } else {
                            self.start = Some(Instant::now());
                            mgr.update_on_timer(Duration::new(0, 0), self.id());
                        }
                    }
                }
                Response::None
            }
        }
        impl kas::WidgetConfig {
            fn configure(&mut self, mgr: &mut Manager) {
                mgr.enable_alt_bypass(true);
            }
        }
        impl Handler {
            type Msg = VoidMsg;
            fn handle(&mut self, mgr: &mut Manager, event: Event) -> Response<VoidMsg> {
                match event {
                    Event::TimerUpdate => {
                        if let Some(start) = self.start {
                            let dur = self.saved + (Instant::now() - start);
                            *mgr += self.display.set_text(format!(
                                "{}.{:03}",
                                dur.as_secs(),
                                dur.subsec_millis()
                            ));
                            mgr.update_on_timer(Duration::new(0, 1), self.id());
                        }
                        Response::None
                    }
                    event => Response::Unhandled(event),
                }
            }
        }
    };

    let mut window = Window::new("Stopwatch", stopwatch);
    window.set_restrict_dimensions(true, true);
    Box::new(window)
}

fn main() -> Result<(), kas_wgpu::Error> {
    env_logger::init();

    let mut theme = kas_theme::ShadedTheme::new();
    let _ = theme.set_colours("dark");
    let mut toolkit = kas_wgpu::Toolkit::new(theme)?;
    toolkit.add_boxed(make_window())?;
    toolkit.run()
}
