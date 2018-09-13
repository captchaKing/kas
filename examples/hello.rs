//! Hello world example

extern crate mygui;

use mygui::widget::canvas::Text;
use mygui::widget::window::{MessageBox, action_close};

use mygui::toolkit::Toolkit;
use mygui::toolkit::gtk::{GtkToolkit, Error};

fn main() -> Result<(), Error> {
    // Build widgets.
    // Message is a Window with an "Ok" button and notification status.
    // Each Window::new method creates objects then solves constraints.
    let window = MessageBox::new(/*Notify::Info,*/
        Text::from("Hello world"),
        action_close);
    
    let mut toolkit = GtkToolkit::new()?;
    toolkit.add(&window);
    toolkit.main();
    Ok(())
}
