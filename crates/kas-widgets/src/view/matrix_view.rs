// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! List view widget

use super::{driver, Driver, PressPhase, SelectionError, SelectionMode};
#[allow(unused)] // doc links
use crate::ScrollBars;
use crate::Scrollable;
use kas::event::components::ScrollComponent;
use kas::event::{ChildMsg, Command, CursorIcon};
use kas::layout::solve_size_rules;
use kas::prelude::*;
use kas::updatable::{MatrixData, UpdatableHandler};
use linear_map::set::LinearSet;
use log::{debug, trace};
use std::time::Instant;
use UpdatableHandler as UpdHandler;

#[derive(Clone, Copy, Debug, Default)]
struct Dim {
    rows: i32,
    cols: i32,
}

#[derive(Clone, Debug, Default)]
struct WidgetData<K, W> {
    key: Option<K>,
    widget: W,
}

widget! {
    /// Matrix view widget
    ///
    /// This widget supports a view over a matrix of shared data items.
    ///
    /// The shared data type `T` must support [`MatrixData`] and
    /// [`UpdatableHandler`], the latter with key type `T::Key` and message type
    /// matching the widget's message. One may use [`kas::updatable::SharedRc`]
    /// or a custom shared data type.
    ///
    /// The driver `V` must implement [`Driver`], with data type
    /// `<T as MatrixData>::Item`. Several implementations are available in the
    /// [`driver`] module or a custom implementation may be used.
    ///
    /// This widget is [`Scrollable`], supporting keyboard, wheel and drag
    /// scrolling. You may wish to wrap this widget with [`ScrollBars`].
    #[derive(Clone, Debug)]
    pub struct MatrixView<
        T: MatrixData + UpdHandler<T::Key, V::Msg> + 'static,
        V: Driver<T::Item> = driver::Default,
    > {
        #[widget_core]
        core: CoreData,
        frame_offset: Offset,
        frame_size: Size,
        view: V,
        data: T,
        widgets: Vec<WidgetData<T::Key, V::Widget>>,
        align_hints: AlignHints,
        ideal_len: Dim,
        alloc_len: Dim,
        cur_len: Dim,
        child_size_min: Size,
        child_size_ideal: Size,
        child_inter_margin: Size,
        child_size: Size,
        scroll: ScrollComponent,
        sel_mode: SelectionMode,
        // TODO(opt): replace selection list with RangeOrSet type?
        selection: LinearSet<T::Key>,
        press_phase: PressPhase,
        press_target: Option<T::Key>,
    }

    impl Self where V: Default {
        /// Construct a new instance
        pub fn new(data: T) -> Self {
            Self::new_with_driver(<V as Default>::default(), data)
        }
    }
    impl Self {
        /// Construct a new instance with explicit view
        pub fn new_with_driver(view: V, data: T) -> Self {
            MatrixView {
                core: Default::default(),
                frame_offset: Default::default(),
                frame_size: Default::default(),
                view,
                data,
                widgets: Default::default(),
                align_hints: Default::default(),
                ideal_len: Dim { rows: 3, cols: 5 },
                alloc_len: Dim::default(),
                cur_len: Dim::default(),
                child_size_min: Size::ZERO,
                child_size_ideal: Size::ZERO,
                child_inter_margin: Size::ZERO,
                child_size: Size::ZERO,
                scroll: Default::default(),
                sel_mode: SelectionMode::None,
                selection: Default::default(),
                press_phase: PressPhase::None,
                press_target: None,
            }
        }

        /// Access the stored data
        pub fn data(&self) -> &T {
            &self.data
        }

        /// Mutably access the stored data
        ///
        /// It may be necessary to use [`MatrixView::update_view`] to update the view of this data.
        pub fn data_mut(&mut self) -> &mut T {
            &mut self.data
        }

        /// Get a copy of the shared value at `key`
        pub fn get_value(&self, key: &T::Key) -> Option<T::Item> {
            self.data.get_cloned(key)
        }

        /// Set shared data
        ///
        /// This method updates the shared data, if supported (see
        /// [`MatrixData::update`]). Other widgets sharing this data are notified
        /// of the update, if data is changed.
        pub fn set_value(&self, mgr: &mut EventMgr, key: &T::Key, data: T::Item) {
            if let Some(handle) = self.data.update(key, data) {
                mgr.trigger_update(handle, 0);
            }
        }

        /// Update shared data
        ///
        /// This is purely a convenience method over [`MatrixView::set_value`].
        /// It does nothing if no value is found at `key`.
        /// It notifies other widgets of updates to the shared data.
        pub fn update_value<F: Fn(T::Item) -> T::Item>(&self, mgr: &mut EventMgr, key: &T::Key, f: F) {
            if let Some(item) = self.get_value(key) {
                self.set_value(mgr, key, f(item));
            }
        }

        /// Get the current selection mode
        pub fn selection_mode(&self) -> SelectionMode {
            self.sel_mode
        }
        /// Set the current selection mode
        pub fn set_selection_mode(&mut self, mode: SelectionMode) -> TkAction {
            self.sel_mode = mode;
            match mode {
                SelectionMode::None if !self.selection.is_empty() => {
                    self.selection.clear();
                    TkAction::REDRAW
                }
                SelectionMode::Single if self.selection.len() > 1 => {
                    if let Some(first) = self.selection.iter().next().cloned() {
                        self.selection.retain(|item| *item == first);
                    }
                    TkAction::REDRAW
                }
                _ => TkAction::empty(),
            }
        }
        /// Set the selection mode (inline)
        #[must_use]
        pub fn with_selection_mode(mut self, mode: SelectionMode) -> Self {
            let _ = self.set_selection_mode(mode);
            self
        }

        /// Read the list of selected entries
        ///
        /// With mode [`SelectionMode::Single`] this may contain zero or one entry;
        /// use `selected_iter().next()` to extract only the first (optional) entry.
        pub fn selected_iter(&'_ self) -> impl Iterator<Item = &'_ T::Key> + '_ {
            self.selection.iter()
        }

        /// Check whether an entry is selected
        pub fn is_selected(&self, key: &T::Key) -> bool {
            self.selection.contains(key)
        }

        /// Clear all selected items
        ///
        /// Does not send [`ChildMsg`] responses.
        pub fn clear_selected(&mut self) -> TkAction {
            if self.selection.is_empty() {
                TkAction::empty()
            } else {
                self.selection.clear();
                TkAction::REDRAW
            }
        }

        /// Directly select an item
        ///
        /// Returns `TkAction::REDRAW` if newly selected, `TkAction::empty()` if
        /// already selected. Fails if selection mode does not permit selection
        /// or if the key is invalid.
        ///
        /// Does not send [`ChildMsg`] responses.
        pub fn select(&mut self, key: T::Key) -> Result<TkAction, SelectionError> {
            match self.sel_mode {
                SelectionMode::None => return Err(SelectionError::Disabled),
                SelectionMode::Single => self.selection.clear(),
                _ => (),
            }
            if !self.data.contains(&key) {
                return Err(SelectionError::Key);
            }
            match self.selection.insert(key) {
                true => Ok(TkAction::REDRAW),
                false => Ok(TkAction::empty()),
            }
        }

        /// Directly deselect an item
        ///
        /// Returns `TkAction::REDRAW` if deselected, `TkAction::empty()` if not
        /// previously selected or if the key is invalid.
        ///
        /// Does not send [`ChildMsg`] responses.
        pub fn deselect(&mut self, key: &T::Key) -> TkAction {
            match self.selection.remove(key) {
                true => TkAction::REDRAW,
                false => TkAction::empty(),
            }
        }

        /// Manually trigger an update to handle changed data
        pub fn update_view(&mut self, mgr: &mut EventMgr) {
            let data = &self.data;
            self.selection.retain(|key| data.contains(key));
            for w in &mut self.widgets {
                w.key = None;
            }
            mgr.set_rect_mgr(|mgr| self.update_widgets(mgr));
            // Force SET_SIZE so that scroll-bar wrappers get updated
            trace!("update_view triggers SET_SIZE");
            *mgr |= TkAction::SET_SIZE;
        }

        /// Set the preferred number of items visible (inline)
        ///
        /// This affects the (ideal) size request and whether children are sized
        /// according to their ideal or minimum size but not the minimum size.
        #[must_use]
        pub fn with_num_visible(mut self, rows: i32, cols: i32) -> Self {
            self.ideal_len = Dim { rows, cols };
            self
        }

        fn update_widgets(&mut self, mgr: &mut SetRectMgr) {
            let time = Instant::now();

            let data_len = Size(self.data.col_len().cast(), self.data.row_len().cast());
            let view_size = self.rect().size;
            let skip = self.child_size + self.child_inter_margin;
            let content_size = (skip.cwise_mul(data_len) - self.child_inter_margin).max(Size::ZERO);
            *mgr |= self.scroll.set_sizes(view_size, content_size);

            let offset = self.scroll_offset();
            let first_col = usize::conv(u64::conv(offset.0) / u64::conv(skip.0));
            let first_row = usize::conv(u64::conv(offset.1) / u64::conv(skip.1));
            let cols = self
                .data
                .col_iter_vec_from(first_col, self.alloc_len.cols.cast());
            let rows = self
                .data
                .row_iter_vec_from(first_row, self.alloc_len.rows.cast());
            self.cur_len = Dim {
                rows: rows.len().cast(),
                cols: cols.len().cast(),
            };

            let pos_start = self.core.rect.pos + self.frame_offset;
            let mut rect = Rect::new(pos_start, self.child_size);

            let mut action = TkAction::empty();
            for (cn, col) in cols.iter().enumerate() {
                let ci = first_col + cn;
                for (rn, row) in rows.iter().enumerate() {
                    let ri = first_row + rn;
                    let i = (ci % cols.len()) + (ri % rows.len()) * cols.len();
                    let w = &mut self.widgets[i];
                    let key = T::make_key(row, col);
                    if w.key.as_ref() != Some(&key) {
                        if let Some(item) = self.data.get_cloned(&key) {
                            w.key = Some(key.clone());
                            action |= self.view.set(&mut w.widget, item);
                        } else {
                            w.key = None; // disables drawing and clicking
                        }
                    }
                    rect.pos = pos_start + skip.cwise_mul(Size(ci.cast(), ri.cast()));
                    if w.widget.rect() != rect {
                        w.widget.set_rect(mgr, rect, self.align_hints);
                    }
                }
            }
            *mgr |= action;
            let dur = (Instant::now() - time).as_micros();
            trace!("MatrixView::update_widgets completed in {}μs", dur);
        }
    }

    impl Scrollable for Self {
        fn scroll_axes(&self, size: Size) -> (bool, bool) {
            let item_min = self.child_size_min + self.child_inter_margin;
            let data_len = Size(self.data.col_len().cast(), self.data.row_len().cast());
            let min_size = (item_min.cwise_mul(data_len) - self.child_inter_margin).max(Size::ZERO);
            (min_size.0 > size.0, min_size.1 > size.1)
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
            mgr.set_rect_mgr(|mgr| self.update_widgets(mgr));
            self.scroll.offset()
        }
    }

    impl WidgetChildren for Self {
        #[inline]
        fn num_children(&self) -> usize {
            self.widgets.len()
        }
        #[inline]
        fn get_child(&self, index: usize) -> Option<&dyn WidgetConfig> {
            self.widgets.get(index).map(|w| w.widget.as_widget())
        }
        #[inline]
        fn get_child_mut(&mut self, index: usize) -> Option<&mut dyn WidgetConfig> {
            self.widgets
                .get_mut(index)
                .map(|w| w.widget.as_widget_mut())
        }
    }

    impl WidgetConfig for Self {
        fn configure(&mut self, mgr: &mut SetRectMgr) {
            if let Some(handle) = self.data.update_handle() {
                mgr.update_on_handle(handle, self.id());
            }
            mgr.register_nav_fallback(self.id());
        }
    }

    impl Layout for Self {
        fn size_rules(&mut self, size_mgr: SizeMgr, axis: AxisInfo) -> SizeRules {
            // We use an invisible frame for highlighting selections, drawing into the margin
            let inner_margin = size_mgr.inner_margin().extract(axis);
            let frame = kas::layout::FrameRules::new_sym(0, inner_margin, 0);

            // We use a default-generated widget to generate size rules
            let mut rules = self.view.make().size_rules(size_mgr.re(), axis);

            self.child_size_min.set_component(axis, rules.min_size());
            self.child_size_ideal
                .set_component(axis, rules.ideal_size());
            let m = rules.margins_i32();
            self.child_inter_margin
                .set_component(axis, (m.0 + m.1).max(inner_margin));

            let ideal_len = match axis.is_vertical() {
                false => self.ideal_len.cols,
                true => self.ideal_len.rows,
            };
            rules.multiply_with_margin(2, ideal_len);
            rules.set_stretch(rules.stretch().max(Stretch::High));

            let (rules, offset, size) = frame.surround_with_margin(rules);
            self.frame_offset.set_component(axis, offset);
            self.frame_size.set_component(axis, size);
            rules
        }

        fn set_rect(&mut self, mgr: &mut SetRectMgr, rect: Rect, align: AlignHints) {
            self.core.rect = rect;

            let mut child_size = rect.size - self.frame_size;
            if child_size.0 >= self.ideal_len.cols * self.child_size_ideal.0 {
                child_size.0 = self.child_size_ideal.0;
            } else {
                child_size.0 = self.child_size_min.0;
            }
            if child_size.1 >= self.ideal_len.rows * self.child_size_ideal.1 {
                child_size.1 = self.child_size_ideal.1;
            } else {
                child_size.1 = self.child_size_min.1;
            }
            self.child_size = child_size;
            self.align_hints = align;

            let skip = child_size + self.child_inter_margin;
            let vis_len = (rect.size + skip - Size::splat(1)).cwise_div(skip) + Size::splat(1);
            self.alloc_len = Dim {
                cols: vis_len.0,
                rows: vis_len.1,
            };

            let old_num = self.widgets.len();
            let num = usize::conv(vis_len.0) * usize::conv(vis_len.1);
            if old_num < num {
                debug!("allocating widgets (old len = {}, new = {})", old_num, num);
                self.widgets.reserve(num - old_num);
                for _ in old_num..num {
                    let id = self.id_ref().make_child(self.widgets.len());
                    let mut widget = self.view.make();
                    mgr.configure(id, &mut widget);
                    solve_size_rules(
                        &mut widget,
                        mgr.size_mgr(),
                        Some(child_size.0),
                        Some(child_size.1),
                    );
                    self.widgets.push(WidgetData { key: None, widget });
                }
            } else if num + 64 <= self.widgets.len() {
                // Free memory (rarely useful?)
                self.widgets.truncate(num);
            }
            self.update_widgets(mgr);
        }

        fn spatial_nav(
            &mut self,
            mgr: &mut SetRectMgr,
            reverse: bool,
            from: Option<usize>,
        ) -> Option<usize> {
            let _ = mgr; // TODO: this needs a rewrite like ListView::spatial_nav

            let cur_len = usize::conv(self.cur_len.cols) * usize::conv(self.cur_len.rows);
            if cur_len == 0 {
                return None;
            }

            // TODO: if last row/col is completely hidden, this and cur_len should be less
            let last = cur_len - 1;

            if let Some(index) = from {
                let p = self.widgets[index].widget.rect().pos;
                let index = match reverse {
                    false if index < last => index + 1,
                    false => 0,
                    true if 0 < index => index - 1,
                    true => last,
                };
                let q = self.widgets[index].widget.rect().pos;
                match reverse {
                    false if q.1 > p.1 || (q.1 == p.1 && q.0 > p.0) => Some(index),
                    true if q.1 < p.1 || (q.1 == p.1 && q.0 < p.0) => Some(index),
                    _ => None,
                }
            } else {
                // Simplified version of logic in update_widgets
                let skip = self.child_size + self.child_inter_margin;
                let offset = self.scroll_offset();
                let ci = usize::conv(u64::conv(offset.0) / u64::conv(skip.0));
                let ri = usize::conv(u64::conv(offset.1) / u64::conv(skip.1));
                let (rows, cols): (usize, usize) = (self.cur_len.rows.cast(), self.cur_len.cols.cast());
                let mut data = (ci % cols) * rows + (ri % rows);
                if reverse {
                    data += last;
                }
                Some(data % cur_len)
            }
        }

        #[inline]
        fn translation(&self) -> Offset {
            self.scroll_offset()
        }

        fn find_id(&mut self, coord: Coord) -> Option<WidgetId> {
            if !self.rect().contains(coord) {
                return None;
            }

            let coord = coord + self.scroll.offset();
            let num = usize::conv(self.cur_len.cols) * usize::conv(self.cur_len.rows);
            for child in &mut self.widgets[..num] {
                if child.key.is_some() {
                    if let Some(id) = child.widget.find_id(coord) {
                        return Some(id);
                    }
                }
            }
            Some(self.id())
        }

        fn draw(&mut self, mut draw: DrawMgr) {
            let mut draw = draw.with_core(self.core_data());
            let offset = self.scroll_offset();
            let num = usize::conv(self.cur_len.cols) * usize::conv(self.cur_len.rows);
            draw.with_clip_region(self.core.rect, offset, |mut draw| {
                for child in &mut self.widgets[..num] {
                    if let Some(ref key) = child.key {
                        child.widget.draw(draw.re());
                        if self.selection.contains(key) {
                            draw.selection_box(child.widget.rect());
                        }
                    }
                }
            });
        }
    }

    impl Handler for Self {
        type Msg = ChildMsg<T::Key, <V::Widget as Handler>::Msg>;

        fn handle(&mut self, mgr: &mut EventMgr, event: Event) -> Response<Self::Msg> {
            match event {
                Event::HandleUpdate { .. } => {
                    self.update_view(mgr);
                    return Response::Update;
                }
                Event::PressMove { coord, .. } => {
                    if let PressPhase::Start(start_coord) = self.press_phase {
                        if mgr.config_test_pan_thresh(coord - start_coord) {
                            self.press_phase = PressPhase::Pan;
                        }
                    }
                    match self.press_phase {
                        PressPhase::Pan => {
                            mgr.update_grab_cursor(self.id(), CursorIcon::Grabbing);
                            // fall through to scroll handler
                        }
                        _ => return Response::Used,
                    }
                }
                Event::PressEnd { ref end_id, .. } => {
                    if self.press_phase == PressPhase::Pan {
                        // fall through to scroll handler
                    } else if end_id.is_some() {
                        if let Some(ref key) = self.press_target {
                            if mgr.config().mouse_nav_focus() {
                                for w in &self.widgets {
                                    if w.key.as_ref().map(|k| k == key).unwrap_or(false) {
                                        if w.widget.key_nav() {
                                            mgr.set_nav_focus(w.widget.id(), false);
                                        }
                                        break;
                                    }
                                }
                            }

                            return match self.sel_mode {
                                SelectionMode::None => Response::Used,
                                SelectionMode::Single => {
                                    mgr.redraw(self.id());
                                    self.selection.clear();
                                    self.selection.insert(key.clone());
                                    ChildMsg::Select(key.clone()).into()
                                }
                                SelectionMode::Multiple => {
                                    mgr.redraw(self.id());
                                    if self.selection.remove(key) {
                                        ChildMsg::Deselect(key.clone()).into()
                                    } else {
                                        self.selection.insert(key.clone());
                                        ChildMsg::Select(key.clone()).into()
                                    }
                                }
                            }
                        }
                        return Response::Used;
                    } else {
                        return Response::Used;
                    }
                }
                Event::Command(cmd, _) => {
                    // Simplified version of logic in update_widgets
                    let (cols, rows): (usize, usize) = (self.cur_len.cols.cast(), self.cur_len.rows.cast());

                    let skip = self.child_size + self.child_inter_margin;
                    let offset = self.scroll_offset();
                    let first_col = usize::conv(u64::conv(offset.0) / u64::conv(skip.0));
                    let first_row = usize::conv(u64::conv(offset.1) / u64::conv(skip.1));
                    let col_start = (first_col / cols) * cols;
                    let row_start = (first_row / rows) * rows;

                    let cur = mgr
                        .nav_focus()
                        .and_then(|id| self.find_child_index(id))
                        .map(|index| {
                            let mut col_index = col_start + index % cols;
                            let mut row_index = row_start + index / cols;
                            if col_index < first_col {
                                col_index += cols;
                            }
                            if row_index < first_row {
                                row_index += rows;
                            }
                            (col_index, row_index)
                        });
                    let last_col = self.data.col_len().wrapping_sub(1);
                    let last_row = self.data.row_len().wrapping_sub(1);

                    let data = match (cmd, cur) {
                        _ if last_col == usize::MAX || last_row == usize::MAX => None,
                        _ if !self.widgets[0].widget.key_nav() => None,
                        (Command::Home, _) => Some((0, 0)),
                        (Command::End, _) => Some((last_col, last_row)),
                        (Command::Left, Some((ci, ri))) if ci > 0 => Some((ci - 1, ri)),
                        (Command::Up, Some((ci, ri))) if ri > 0 => Some((ci, ri - 1)),
                        (Command::Right, Some((ci, ri))) if ci < last_col => Some((ci + 1, ri)),
                        (Command::Down, Some((ci, ri))) if ri < last_row => Some((ci, ri + 1)),
                        (Command::PageUp, Some((ci, ri))) if ri > 0 => {
                            Some((ci, ri.saturating_sub(rows / 2)))
                        }
                        (Command::PageDown, Some((ci, ri))) if ri < last_row => {
                            Some((ci, (ri + rows / 2).min(last_row)))
                        }
                        _ => None,
                    };
                    if let Some((ci, ri)) = data {
                        // Set nav focus to index and update scroll position
                        // Note: we update nav focus before updating widgets; this is fine
                        let index = (ci % cols) + (ri % rows) * cols;
                        mgr.set_nav_focus(self.widgets[index].widget.id(), true);
                    }
                    return Response::Used;
                }
                _ => (), // fall through to scroll handler
            }

            let self_id = self.id();
            let (action, response) = self.scroll
                .scroll_by_event(mgr, event, self.id(), self.core.rect.size, |mgr, source, _, coord| {
                    if source.is_primary() && mgr.config_enable_mouse_pan() {
                        let icon = Some(CursorIcon::Grabbing);
                        mgr.grab_press_unique(self_id, source, coord, icon);
                    }
                });

            if !action.is_empty() {
                *mgr |= action;
                mgr.set_rect_mgr(|mgr| self.update_widgets(mgr));
                Response::Focus(self.rect())
            } else {
                response.void_into()
            }
        }
    }

    impl SendEvent for Self {
        fn send(&mut self, mgr: &mut EventMgr, id: WidgetId, event: Event) -> Response<Self::Msg> {
            if self.is_disabled() {
                return Response::Unused;
            }

            if let Some(index) = self.id().index_of_child(&id) {
                let child_event = self.scroll.offset_event(event.clone());
                let response;
                if let Some(child) = self.widgets.get_mut(index) {
                    let r = child.widget.send(mgr, id, child_event);
                    response = (child.key.clone(), r);
                } else {
                    return Response::Unused;
                };

                if matches!(&response.1, Response::Update | Response::Msg(_)) {
                    let wd = &self.widgets[index];
                    if let Some(key) = wd.key.as_ref() {
                        if let Some(value) = self.view.get(&wd.widget) {
                            if let Some(handle) = self.data.update(key, value) {
                                mgr.trigger_update(handle, 0);
                            }
                        }
                    }
                }

                match response {
                    (key, Response::Unused) => {
                        if let Event::PressStart { source, coord, .. } = event {
                            if source.is_primary() {
                                // We request a grab with our ID, hence the
                                // PressMove/PressEnd events are matched in handle().
                                mgr.grab_press_unique(self.id(), source, coord, None);
                                self.press_phase = PressPhase::Start(coord);
                                self.press_target = key;
                                Response::Used
                            } else {
                                Response::Unused
                            }
                        } else {
                            self.handle(mgr, event)
                        }
                    }
                    (_, Response::Used) => Response::Used,
                    (_, Response::Pan(delta)) => match self.scroll_by_delta(mgr, delta) {
                        delta if delta == Offset::ZERO => Response::Scrolled,
                        delta => Response::Pan(delta),
                    }
                    (_, Response::Scrolled) => Response::Scrolled,
                    (_, Response::Focus(rect)) => {
                        let (rect, action) = self.scroll.focus_rect(rect, self.core.rect);
                        *mgr |= action;
                        mgr.set_rect_mgr(|mgr| self.update_widgets(mgr));
                        Response::Focus(rect)
                    }
                    (Some(key), Response::Select) => {
                        match self.sel_mode {
                            SelectionMode::None => Response::Used,
                            SelectionMode::Single => {
                                mgr.redraw(self.id());
                                self.selection.clear();
                                self.selection.insert(key.clone());
                                Response::Msg(ChildMsg::Select(key))
                            }
                            SelectionMode::Multiple => {
                                mgr.redraw(self.id());
                                if self.selection.remove(&key) {
                                    Response::Msg(ChildMsg::Deselect(key))
                                } else {
                                    self.selection.insert(key.clone());
                                    Response::Msg(ChildMsg::Select(key))
                                }
                            }
                        }
                    }
                    (None, Response::Select) => Response::Used,
                    (_, Response::Update) => Response::Used,
                    (key, Response::Msg(msg)) => {
                        trace!(
                            "Received by {} from {:?}: {:?}",
                            self.id(),
                            &key,
                            kas::util::TryFormat(&msg)
                        );
                        if let Some(key) = key {
                            if let Some(handle) = self.data.handle(&key, &msg) {
                                mgr.trigger_update(handle, 0);
                            }
                            Response::Msg(ChildMsg::Child(key, msg))
                        } else {
                            log::warn!("MatrixView: response from widget with no key");
                            Response::Used
                        }
                    }
                }
            } else {
                debug_assert!(self.eq_id(id), "SendEvent::send: bad WidgetId");
                self.handle(mgr, event)
            }
        }
    }
}
