// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Message Map widget

use crate::prelude::*;
use std::fmt;
use std::rc::Rc;

/// Wrapper to map messages from the inner widget
#[derive(Clone, Widget)]
#[layout(single)]
#[handler(msg=M, send=noauto)]
pub struct MsgMapWidget<W: Widget, M: 'static> {
    #[widget_core]
    core: kas::CoreData,
    #[widget]
    inner: W,
    map: Rc<dyn Fn(&mut Manager, W::Msg) -> Response<M>>,
}

impl<W: Widget, M> MsgMapWidget<W, M> {
    /// Construct
    ///
    /// Any response from the child widget with a message payload is mapped
    /// through the closure `f`.
    pub fn new<F: Fn(&mut Manager, W::Msg) -> Response<M> + 'static>(child: W, f: F) -> Self {
        Self::new_rc(child, Rc::new(f))
    }

    /// Construct with an Rc-wrapped method
    ///
    /// Any response from the child widget with a message payload is mapped
    /// through the closure `f`.
    pub fn new_rc(child: W, f: Rc<dyn Fn(&mut Manager, W::Msg) -> Response<M>>) -> Self {
        MsgMapWidget {
            core: Default::default(),
            inner: child,
            map: f,
        }
    }
}

impl<W: Widget, M> fmt::Debug for MsgMapWidget<W, M> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MsgMapWidget {{ core: {:?}, inner: {:?}, ... }}",
            self.core, self.inner,
        )
    }
}

impl<W: Widget, M> SendEvent for MsgMapWidget<W, M> {
    fn send(&mut self, mgr: &mut Manager, id: WidgetId, event: Event) -> Response<Self::Msg> {
        if self.is_disabled() {
            return Response::Unhandled;
        }

        if id < self.id() {
            let r = self.inner.send(mgr, id, event);
            r.try_into().unwrap_or_else(|msg| (self.map)(mgr, msg))
        } else {
            debug_assert!(id == self.id(), "SendEvent::send: bad WidgetId");
            self.handle(mgr, event)
        }
    }
}
