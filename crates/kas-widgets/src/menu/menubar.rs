// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Menubar

use super::{Menu, SubMenu};
use crate::IndexedList;
use kas::event::{self, Command};
use kas::prelude::*;

widget! {
    /// A menu-bar
    ///
    /// This widget houses a sequence of menu buttons, allowing input actions across
    /// menus.
    #[derive(Clone, Debug)]
    #[widget{
        layout = single;
    }]
    pub struct MenuBar<W: Menu, D: Directional = kas::dir::Right> {
        #[widget_core]
        core: CoreData,
        #[widget]
        pub bar: IndexedList<D, SubMenu<D::Flipped, W>>,
        // Open mode. Used to close with click on root only when previously open.
        opening: bool,
        delayed_open: Option<WidgetId>,
    }

    impl Self where D: Default {
        /// Construct a menubar
        ///
        /// Note: it appears that `MenuBar::new(..)` causes a type inference error,
        /// however `MenuBar::<_>::new(..)` does not. Alternatively one may specify
        /// the direction explicitly: `MenuBar::<_, kas::dir::Right>::new(..)`.
        pub fn new(menus: Vec<SubMenu<D::Flipped, W>>) -> Self {
            MenuBar::new_with_direction(D::default(), menus)
        }
    }

    impl Self {
        /// Construct a menubar with explicit direction
        pub fn new_with_direction(direction: D, mut menus: Vec<SubMenu<D::Flipped, W>>) -> Self {
            for menu in menus.iter_mut() {
                menu.key_nav = false;
            }
            MenuBar {
                core: Default::default(),
                bar: IndexedList::new_with_direction(direction, menus),
                opening: false,
                delayed_open: None,
            }
        }
    }

    impl<W: Menu<Msg = M>, D: Directional, M: 'static> event::Handler for MenuBar<W, D> {
        type Msg = M;

        fn handle(&mut self, mgr: &mut EventMgr, event: Event) -> Response<Self::Msg> {
            match event {
                Event::TimerUpdate(id_code) => {
                    if let Some(id) = self.delayed_open.clone() {
                        if id.as_u64() == id_code {
                            self.set_menu_path(mgr, Some(&id), false);
                        }
                    }
                    Response::Used
                }
                Event::PressStart {
                    source,
                    start_id,
                    coord,
                } => {
                    if start_id.as_ref().map(|id| self.is_ancestor_of(id)).unwrap_or(false) {
                        if source.is_primary() {
                            mgr.grab_press_unique(self.id(), source, coord, None);
                            mgr.set_grab_depress(source, start_id.clone());
                            self.opening = false;
                            if self.rect().contains(coord) {
                                if self
                                    .bar
                                    .iter()
                                    .any(|w| w.eq_id(&start_id) && !w.menu_is_open())
                                {
                                    self.opening = true;
                                    self.set_menu_path(mgr, start_id.as_ref(), false);
                                } else {
                                    self.set_menu_path(mgr, None, false);
                                }
                            } else {
                                let delay = mgr.config().menu_delay();
                                mgr.update_on_timer(delay, self.id(), WidgetId::opt_to_u64(start_id.as_ref()));
                                self.delayed_open = start_id;
                            }
                        }
                        Response::Used
                    } else {
                        self.delayed_open = None;
                        Response::Unused
                    }
                }
                Event::PressMove {
                    source,
                    cur_id,
                    coord,
                    ..
                } => {
                    mgr.set_grab_depress(source, cur_id.clone());
                    if let Some(id) = cur_id {
                        if self.bar.is_strict_ancestor_of(&id) {
                            // We instantly open a sub-menu on motion over the bar,
                            // but delay when over a sub-menu (most intuitive?)
                            if self.rect().contains(coord) && !self.bar.eq_id(&id) {
                                self.set_menu_path(mgr, Some(&id), false);
                            } else if id != self.delayed_open {
                                mgr.set_nav_focus(id.clone(), false);
                                let delay = mgr.config().menu_delay();
                                mgr.update_on_timer(delay, self.id(), id.as_u64());
                                self.delayed_open = Some(id);
                            }
                        }
                    }
                    Response::Used
                }
                Event::PressEnd { coord, end_id, success, .. } if success => {
                    if end_id.as_ref().map(|id| self.is_ancestor_of(id)).unwrap_or(false) {
                        // end_id is a child of self
                        let id = end_id.unwrap();

                        if self.rect().contains(coord) {
                            // end coordinate is on the menubar
                            if !self.opening {
                                self.delayed_open = None;
                                for i in 0..self.bar.len() {
                                    if self.bar[i].eq_id(&id) {
                                        self.bar[i].set_menu_path(mgr, None, false);
                                    }
                                }
                            }
                        } else {
                            // not on the menubar, thus on a sub-menu
                            self.delayed_open = None;
                            return self.send(mgr, id, Event::Activate);
                        }
                    } else {
                        // not on the menu
                        self.set_menu_path(mgr, None, false);
                    }
                    Response::Used
                }
                Event::PressEnd { .. } => Response::Used,
                Event::Command(cmd, _) => {
                    // Arrow keys can switch to the next / previous menu
                    // as well as to the first / last item of an open menu.
                    use Command::{Left, Up};
                    let is_vert = self.bar.direction().is_vertical();
                    let reverse = self.bar.direction().is_reversed() ^ matches!(cmd, Left | Up);
                    match cmd.as_direction().map(|d| d.is_vertical()) {
                        Some(v) if v == is_vert => {
                            for i in 0..self.bar.len() {
                                if self.bar[i].menu_is_open() {
                                    let mut j = isize::conv(i);
                                    j = if reverse { j - 1 } else { j + 1 };
                                    j = j.rem_euclid(self.bar.len().cast());
                                    self.bar[i].set_menu_path(mgr, None, true);
                                    let w = &mut self.bar[usize::conv(j)];
                                    w.set_menu_path(mgr, Some(&w.id()), true);
                                    break;
                                }
                            }
                            Response::Used
                        }
                        Some(_) => {
                            mgr.next_nav_focus(self, reverse, true);
                            Response::Used
                        }
                        None => Response::Unused,
                    }
                }
                _ => Response::Unused,
            }
        }
    }

    impl event::SendEvent for Self {
        fn send(&mut self, mgr: &mut EventMgr, id: WidgetId, event: Event) -> Response<Self::Msg> {
            if self.is_disabled() {
                return Response::Unused;
            }

            if self.eq_id(&id) {
                self.handle(mgr, event)
            } else {
                match self.bar.send(mgr, id.clone(), event.clone()) {
                    Response::Unused => self.handle(mgr, event),
                    r => r.try_into().unwrap_or_else(|(_, msg)| {
                        log::trace!(
                            "Received by {} from {}: {:?}",
                            self.id(),
                            id,
                            kas::util::TryFormat(&msg)
                        );
                        Response::Msg(msg)
                    }),
                }
            }
        }
    }

    impl Menu for Self {
        fn set_menu_path(&mut self, mgr: &mut EventMgr, target: Option<&WidgetId>, set_focus: bool) {
            log::trace!("{}::set_menu_path: target={:?}, set_focus={}", self.identify(), target, set_focus);
            self.delayed_open = None;
            for i in 0..self.bar.len() {
                self.bar[i].set_menu_path(mgr, target, set_focus);
            }
        }
    }
}
