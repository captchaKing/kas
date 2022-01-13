// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Scroll region

use super::Scrollable;
use kas::event::ScrollDelta::{LineDelta, PixelDelta};
use kas::event::{self, Command, PressSource};
use kas::geom::Vec2;
use kas::prelude::*;
use kas::theme::TextClass;
use std::fmt::Debug;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
enum Glide {
    None,
    Drag(u8, [(Instant, Offset); 4]),
    Glide(Instant, Vec2, Vec2),
}

impl Glide {
    fn move_delta(&mut self, delta: Offset) {
        match self {
            Glide::Drag(next, samples) => {
                samples[*next as usize] = (Instant::now(), delta);
                *next = (*next + 1) % 4;
            }
            _ => {
                let x = (Instant::now(), delta);
                *self = Glide::Drag(1, [x; 4]);
            }
        }
    }

    fn opt_start(&mut self, timeout: Duration) -> bool {
        if let Glide::Drag(_, samples) = self {
            let now = Instant::now();
            let start = now - timeout;
            let mut delta = Offset::ZERO;
            let mut t0 = now;
            for (time, d) in samples {
                if *time >= start {
                    t0 = t0.min(*time);
                    delta += *d;
                }
            }
            let dur = now - t0;
            let v = Vec2::from(delta) / dur.as_secs_f32();
            if dur >= Duration::from_millis(1) && v != Vec2::ZERO {
                *self = Glide::Glide(now, v, Vec2::ZERO);
                true
            } else {
                *self = Glide::None;
                false
            }
        } else {
            false
        }
    }

    fn step(&mut self, (decay_mul, decay_sub): (f32, f32)) -> Option<Offset> {
        if let Glide::Glide(start, v, rest) = self {
            let now = Instant::now();
            let dur = (now - *start).as_secs_f32();
            let d = *v * dur + *rest;
            let rest = d.fract();
            let delta = Offset::from(d.trunc());

            if v.max_abs_comp() >= 1.0 {
                let mut v = *v * decay_mul.powf(dur);
                v = v - v.abs().min(Vec2::splat(decay_sub * dur)) * v.sign();
                *self = Glide::Glide(now, v, rest);
                Some(delta)
            } else {
                *self = Glide::None;
                None
            }
        } else {
            None
        }
    }
}

/// Logic for a scroll region
///
/// This struct handles some scroll logic. It does not provide scrollbars.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollComponent {
    max_offset: Offset,
    offset: Offset,
    scroll_rate: f32,
    glide: Glide,
}

impl Default for ScrollComponent {
    #[inline]
    fn default() -> Self {
        ScrollComponent {
            max_offset: Offset::ZERO,
            offset: Offset::ZERO,
            scroll_rate: 30.0,
            glide: Glide::None,
        }
    }
}

impl ScrollComponent {
    /// Get the maximum offset
    ///
    /// Note: the minimum offset is always zero.
    #[inline]
    pub fn max_offset(&self) -> Offset {
        self.max_offset
    }

    /// Get the current offset
    ///
    /// To translate a coordinate from the outer region to a coordinate of the
    /// scrolled region, add this offset.
    #[inline]
    pub fn offset(&self) -> Offset {
        self.offset
    }

    /// Set sizes:
    ///
    /// -   `window_size`: size of scroll region on the outside
    /// -   `content_size`: size of scroll region on the inside (usually larger)
    ///
    /// Like [`Self::set_offset`] this generates a [`TkAction`] due to potential
    /// change in offset. In practice the caller will likely be performing all
    /// required updates regardless and the return value can be safely ignored.
    pub fn set_sizes(&mut self, window_size: Size, content_size: Size) -> TkAction {
        self.max_offset = Offset::from(content_size) - Offset::from(window_size);
        self.set_offset(self.offset)
    }

    /// Set the scroll offset
    ///
    /// The offset is clamped to the available scroll range.
    /// Returns [`TkAction::empty()`] if the offset is identical to the old offset,
    /// or [`TkAction::REGION_MOVED`] if the offset changes.
    #[inline]
    pub fn set_offset(&mut self, offset: Offset) -> TkAction {
        let offset = offset.clamp(Offset::ZERO, self.max_offset);
        if offset == self.offset {
            TkAction::empty()
        } else {
            self.offset = offset;
            TkAction::REGION_MOVED
        }
    }

    /// Set the scroll rate
    ///
    /// This affects how fast arrow keys and the mouse wheel scroll (but not
    /// pixel offsets, as from touch devices supporting smooth scrolling).
    #[inline]
    pub fn set_scroll_rate(&mut self, rate: f32) {
        self.scroll_rate = rate;
    }

    /// Apply offset to an event being sent to the scrolled child
    #[inline]
    pub fn offset_event(&self, mut event: Event) -> Event {
        match &mut event {
            Event::PressStart { coord, .. } => {
                *coord += self.offset;
            }
            Event::PressMove { coord, .. } => {
                *coord += self.offset;
            }
            Event::PressEnd { coord, .. } => {
                *coord += self.offset;
            }
            _ => {}
        };
        event
    }

    /// Handle [`Response::Focus`]
    ///
    /// Inputs and outputs:
    ///
    /// -   `rect`: the focus rect
    /// -   `window_rect`: the rect of the scroll window
    /// -   returned `Rect`: the focus rect, adjusted for scroll offset; normally this should be
    ///     returned via another [`Response::Focus`]
    /// -   returned `TkAction`: action to pass to the event manager
    #[inline]
    pub fn focus_rect(&mut self, rect: Rect, window_rect: Rect) -> (Rect, TkAction) {
        let v = rect.pos - window_rect.pos;
        let off = Offset::from(rect.size) - Offset::from(window_rect.size);
        let offset = self.offset.max(v + off).min(v);
        let action = self.set_offset(offset);
        (rect - self.offset, action)
    }

    /// Use an event to scroll, if possible
    ///
    /// Handles keyboard (Home/End, Page Up/Down and arrow keys), mouse wheel
    /// and touchpad scroll events. Also handles mouse/touch drag events *if*
    /// the `on_press_start` closure activates a mouse/touch grab.
    ///
    /// Behaviour on [`Event::PressStart`] is configurable: the closure is called on
    /// this event and should call [`EventMgr::request_grab`] if the press should
    /// scroll by drag. This allows control of which mouse button(s) are used and
    /// whether any modifiers must be pressed. For example:
    /// ```
    /// # use kas::prelude::*;
    /// # type Msg = ();
    /// fn dummy_event_handler(
    ///     id: WidgetId,
    ///     scroll: &mut kas_widgets::ScrollComponent,
    ///     mgr: &mut EventMgr,
    ///     event: Event
    /// )
    ///     -> Response<Msg>
    /// {
    ///     let window_size = Size(100, 80);
    ///     let (action, response) = scroll.scroll_by_event(event, window_size, |source, _, coord| {
    ///         if source.is_primary() {
    ///             let icon = Some(kas::event::CursorIcon::Grabbing);
    ///             mgr.request_grab(id, source, coord, kas::event::GrabMode::Grab, icon);
    ///         }
    ///     });
    ///     *mgr |= action;
    ///     response.void_into()
    /// }
    /// ```
    ///
    /// If the returned [`TkAction`] is `None`, the scroll offset has not changed and
    /// the returned [`Response`] is either `Used` or `Unused`.
    /// If the returned [`TkAction`] is not `None`, the scroll offset has been
    /// updated and the second return value is `Response::Used`.
    #[inline]
    pub fn scroll_by_event<PS: FnOnce(&mut EventMgr, PressSource, WidgetId, Coord)>(
        &mut self,
        mgr: &mut EventMgr,
        event: Event,
        id: WidgetId,
        window_size: Size,
        on_press_start: PS,
    ) -> (TkAction, Response<VoidMsg>) {
        let mut action = TkAction::empty();
        let mut response = Response::Used;

        match event {
            Event::TimerUpdate(0) => {
                // Momentum/glide scrolling: update per arbitrary step time until movment stops.
                let decay = mgr.config().scroll_flick_decay();
                if let Some(delta) = self.glide.step(decay) {
                    action = self.set_offset(self.offset - delta);
                    mgr.update_on_timer(Duration::from_millis(3), id, 0);
                    response = Response::Scrolled;
                }
            }
            Event::Command(Command::Home, _) => {
                action = self.set_offset(Offset::ZERO);
            }
            Event::Command(Command::End, _) => {
                action = self.set_offset(self.max_offset);
            }
            Event::Command(cmd, _) => {
                let delta = match cmd {
                    Command::Left => LineDelta(-1.0, 0.0),
                    Command::Right => LineDelta(1.0, 0.0),
                    Command::Up => LineDelta(0.0, 1.0),
                    Command::Down => LineDelta(0.0, -1.0),
                    Command::PageUp => PixelDelta(Offset(0, window_size.1 / 2)),
                    Command::PageDown => PixelDelta(Offset(0, -(window_size.1 / 2))),
                    _ => return (action, Response::Unused),
                };

                let d = match delta {
                    LineDelta(x, y) => Offset(
                        (-self.scroll_rate * x).cast_nearest(),
                        (self.scroll_rate * y).cast_nearest(),
                    ),
                    PixelDelta(d) => d,
                };
                action = self.set_offset(self.offset - d);
            }
            Event::Scroll(delta) => {
                let d = match delta {
                    LineDelta(x, y) => Offset(
                        (-self.scroll_rate * x).cast_nearest(),
                        (self.scroll_rate * y).cast_nearest(),
                    ),
                    PixelDelta(d) => d,
                };
                let old_offset = self.offset;
                action = self.set_offset(old_offset - d);
                let delta = d - (old_offset - self.offset);
                response = if delta != Offset::ZERO {
                    Response::Pan(delta)
                } else {
                    Response::Scrolled
                };
            }
            Event::PressStart {
                source,
                start_id,
                coord,
            } => on_press_start(mgr, source, start_id, coord),
            Event::PressMove { mut delta, .. } => {
                self.glide.move_delta(delta);
                let old_offset = self.offset;
                action = self.set_offset(old_offset - delta);
                delta = old_offset - self.offset;
                response = if delta != Offset::ZERO {
                    Response::Pan(delta)
                } else {
                    Response::Scrolled
                };
            }
            Event::PressEnd { .. } => {
                if self.glide.opt_start(mgr.config().scroll_flick_timeout()) {
                    mgr.update_on_timer(Duration::new(0, 0), id, 0);
                }
            }
            _ => response = Response::Unused,
        }
        (action, response)
    }
}

widget! {
    /// A scrollable region
    ///
    /// This region supports scrolling via mouse wheel and click/touch drag.
    ///
    /// Scrollbars are not included; use [`ScrollBarRegion`] if you want those.
    ///
    /// [`ScrollBarRegion`]: crate::ScrollBarRegion
    #[autoimpl(Deref, DerefMut on inner)]
    #[autoimpl(class_traits where W: trait on inner)]
    #[derive(Clone, Debug, Default)]
    #[handler(msg = <W as event::Handler>::Msg)]
    pub struct ScrollRegion<W: Widget> {
        #[widget_core]
        core: CoreData,
        min_child_size: Size,
        offset: Offset,
        frame_size: Size,
        scroll: ScrollComponent,
        #[widget]
        inner: W,
    }

    impl Self {
        /// Construct a new scroll region around an inner widget
        #[inline]
        pub fn new(inner: W) -> Self {
            ScrollRegion {
                core: Default::default(),
                min_child_size: Size::ZERO,
                offset: Default::default(),
                frame_size: Default::default(),
                scroll: Default::default(),
                inner,
            }
        }

        /// Access inner widget directly
        #[inline]
        pub fn inner(&self) -> &W {
            &self.inner
        }

        /// Access inner widget directly
        #[inline]
        pub fn inner_mut(&mut self) -> &mut W {
            &mut self.inner
        }
    }

    impl Scrollable for Self {
        fn scroll_axes(&self, size: Size) -> (bool, bool) {
            (
                self.min_child_size.0 > size.0,
                self.min_child_size.1 > size.1,
            )
        }

        #[inline]
        fn max_scroll_offset(&self) -> Offset {
            self.scroll.max_offset()
        }

        #[inline]
        fn scroll_offset(&self) -> Offset {
            self.scroll.offset()
        }

        #[inline]
        fn set_scroll_offset(&mut self, mgr: &mut EventMgr, offset: Offset) -> Offset {
            *mgr |= self.scroll.set_offset(offset);
            self.scroll.offset()
        }
    }

    impl WidgetConfig for Self {
        fn configure(&mut self, mgr: &mut EventMgr) {
            mgr.register_nav_fallback(self.id());
        }
    }

    impl Layout for Self {
        fn size_rules(&mut self, size_mgr: SizeMgr, axis: AxisInfo) -> SizeRules {
            let mut rules = self.inner.size_rules(size_mgr.re(), axis);
            self.min_child_size.set_component(axis, rules.min_size());
            let line_height = size_mgr.line_height(TextClass::Label);
            self.scroll.set_scroll_rate(3.0 * f32::conv(line_height));
            rules.reduce_min_to(line_height);

            // We use a zero-sized frame to push any margins inside the scroll-region.
            let frame = kas::layout::FrameRules::new(0, 0, 0, (0, 0));
            let (rules, offset, size) = frame.surround_with_margin(rules);
            self.offset.set_component(axis, offset);
            self.frame_size.set_component(axis, size);
            rules
        }

        fn set_rect(&mut self, mgr: &mut SetRectMgr, rect: Rect, align: AlignHints) {
            self.core.rect = rect;
            let child_size = (rect.size - self.frame_size).max(self.min_child_size);
            let child_rect = Rect::new(rect.pos + self.offset, child_size);
            self.inner.set_rect(mgr, child_rect, align);
            let _ = self
                .scroll
                .set_sizes(rect.size, child_size + self.frame_size);
        }

        fn find_id(&mut self, coord: Coord) -> Option<WidgetId> {
            if !self.rect().contains(coord) {
                return None;
            }
            self.inner.find_id(coord + self.translation())
        }

        #[inline]
        fn translation(&self) -> Offset {
            self.scroll_offset()
        }

        fn draw(&mut self, mut draw: DrawMgr, disabled: bool) {
            let disabled = disabled || self.is_disabled();
            draw.with_clip_region(self.core.rect, self.scroll_offset(), |handle| {
                self.inner.draw(handle, disabled)
            });
        }
    }

    impl event::SendEvent for Self {
        fn send(&mut self, mgr: &mut EventMgr, id: WidgetId, event: Event) -> Response<Self::Msg> {
            if self.is_disabled() {
                return Response::Unused;
            }

            if self.inner.id().is_ancestor_of(&id) {
                let child_event = self.scroll.offset_event(event.clone());
                match self.inner.send(mgr, id, child_event) {
                    Response::Unused => (),
                    Response::Pan(delta) => {
                        return match self.scroll_by_delta(mgr, delta) {
                            delta if delta == Offset::ZERO => Response::Scrolled,
                            delta => Response::Pan(delta),
                        };
                    }
                    Response::Focus(rect) => {
                        let (rect, action) = self.scroll.focus_rect(rect, self.core.rect);
                        *mgr |= action;
                        return Response::Focus(rect);
                    }
                    r => return r,
                }
            } else {
                debug_assert!(self.eq_id(id), "SendEvent::send: bad WidgetId");
            };

            let id = self.id();
            let (action, response) =
                self.scroll
                    .scroll_by_event(mgr, event, self.id(), self.core.rect.size, |mgr, source, _, coord| {
                        if source.is_primary() && mgr.config_enable_mouse_pan() {
                            let icon = Some(event::CursorIcon::Grabbing);
                            mgr.request_grab(id, source, coord, event::GrabMode::Grab, icon);
                        }
                    });
            if !action.is_empty() {
                *mgr |= action;
                Response::Focus(self.core.rect)
            } else {
                response.void_into()
            }
        }
    }
}
