//! Counter example (simple button)
#![feature(unrestricted_attribute_tokens)]
#![feature(proc_macro_hygiene)]

use std::fmt::Write;
use std::time::{Duration, Instant};

use mygui::control::TextButton;
use mygui::display::Text;
use mygui::event::{Handler, NoResponse};
use mygui::macros::{NoResponse, Widget};
use mygui::{Class, CoreData, Widget, SimpleWindow, Toolkit, TkWidget, Window, CallbackCond};

#[derive(Debug, NoResponse)]
enum Control {
    None,
    Reset,
    Start,
}

fn main() -> Result<(), mygui_gtk::Error> {
    #[layout(horizontal)]
    #[widget(class = Class::Container)]
    #[handler(response = NoResponse, generics = <>
        where BR: Handler<Response = Control>, BS: Handler<Response = Control>)]
    #[derive(Clone, Debug, Widget)]
    struct Stopwatch<BR: Widget, BS: Widget> {
        #[core] core: CoreData,
        #[widget] display: Text,
        #[widget(handler = handle_button)] b_reset: BR,
        #[widget(handler = handle_button)] b_start: BS,
        saved: Duration,
        start: Option<Instant>,
        dur_buf: String,
    }
    
    impl<BR: Widget, BS: Widget> Stopwatch<BR, BS> {
        fn handle_button(&mut self, _tk: &TkWidget, msg: Control) -> NoResponse {
            match msg {
                Control::None => {}
                Control::Reset => {
                    self.saved = Duration::default();
                    self.start = None;
                }
                Control::Start => {
                    if let Some(start) = self.start {
                        self.saved += Instant::now() - start;
                        self.start = None;
                    } else {
                        self.start = Some(Instant::now());
                    }
                }
            }
            NoResponse
        }
        
        fn on_tick(&mut self, tk: &TkWidget) {
            if let Some(start) = self.start {
                let dur = self.saved + (Instant::now() - start);
                self.dur_buf.clear();
                self.dur_buf.write_fmt(format_args!(
                    "{}.{:03}",
                    dur.as_secs(),
                    dur.subsec_millis()
                )).unwrap();
                println!("dur: {}", &self.dur_buf);
                self.display.set_text(tk, &self.dur_buf);
            }
        }
    }
    
    let stopwatch = Stopwatch {
        core: CoreData::default(),
        display: Text::from("0.000"),
        b_reset: TextButton::new("⏮", || Control::Reset),
        b_start: TextButton::new("⏯", || Control::Start),
        saved: Duration::default(),
        start: None,
        dur_buf: String::default(),
    };
    
    let mut window = SimpleWindow::new(stopwatch);
    
    window.add_callback(CallbackCond::TimeoutMs(100),
            |window, tk| window.get_mut().on_tick(tk) );
    
    let mut toolkit = mygui_gtk::Toolkit::new()?;
    toolkit.add(window);
    toolkit.main();
    Ok(())
}
