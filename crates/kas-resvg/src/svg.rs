// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! SVG widget

// TODO: error handling (unwrap)

use kas::draw::{ImageFormat, ImageId};
use kas::geom::Vec2;
use kas::layout::MarginSelector;
use kas::prelude::*;
use std::path::PathBuf;
use tiny_skia::Pixmap;

widget! {
    /// An SVG image loaded from a path
    #[cfg_attr(doc_cfg, doc(cfg(feature = "svg")))]
    #[autoimpl(Debug skip self.tree)]
    #[derive(Clone)]
    pub struct Svg {
        #[widget_core]
        core: CoreData,
        path: PathBuf,
        tree: Option<usvg::Tree>,
        margins: MarginSelector,
        min_size_factor: f32,
        ideal_size_factor: f32,
        min_size: Size,
        ideal_size: Size,
        stretch: Stretch,
        pixmap: Option<Pixmap>,
        image_id: Option<ImageId>,
    }

    impl Svg {
        /// Construct with a path and size factors
        ///
        /// An SVG image has an embedded "original" size. This constructor
        /// multiplies that size by the given factors to obtain minimum and ideal
        /// sizes (see [`SizeRules`] for a description of min / ideal sizes).
        pub fn from_path_and_factors<P: Into<PathBuf>>(
            path: P,
            min_size_factor: f32,
            ideal_size_factor: f32,
        ) -> Self {
            Svg {
                core: Default::default(),
                path: path.into(),
                tree: None,
                margins: MarginSelector::Outer,
                min_size_factor,
                ideal_size_factor,
                min_size: Size::ZERO,
                ideal_size: Size::ZERO,
                stretch: Stretch::Low,
                pixmap: None,
                image_id: None,
            }
        }

        /// Set margins
        #[must_use]
        pub fn with_margins(mut self, margins: MarginSelector) -> Self {
            self.margins = margins;
            self
        }

        /// Set stretch policy
        #[must_use]
        pub fn with_stretch(mut self, stretch: Stretch) -> Self {
            self.stretch = stretch;
            self
        }

        /// Set margins
        pub fn set_margins(&mut self, margins: MarginSelector) {
            self.margins = margins;
        }

        /// Set stretch policy
        pub fn set_stretch(&mut self, stretch: Stretch) {
            self.stretch = stretch;
        }
    }

    impl WidgetConfig for Svg {
        fn configure(&mut self, mgr: &mut SetRectMgr) {
            if self.tree.is_none() {
                // TODO: maybe we should use a singleton to deduplicate loading by
                // path? Probably not much use for duplicate SVG widgets however.
                let data = std::fs::read(&self.path).unwrap();
                let scale_factor = mgr.scale_factor();
                let def_size = 100.0 * f64::conv(scale_factor);
                let fonts_db = kas::text::fonts::fonts().read_db();
                let fontdb = fonts_db.db();
                let font_family = fonts_db
                    .font_family_from_alias("SERIF")
                    .unwrap_or_default();
                let font_size = mgr.size_mgr().pixels_from_em(1.0) as f64;

                // TODO: some options here should be configurable
                let opts = usvg::OptionsRef {
                    resources_dir: self.path.parent(),
                    dpi: 96.0 * f64::conv(scale_factor),
                    font_family: &font_family,
                    font_size,
                    languages: &[],
                    shape_rendering: usvg::ShapeRendering::default(),
                    text_rendering: usvg::TextRendering::default(),
                    image_rendering: usvg::ImageRendering::default(),
                    keep_named_groups: false,
                    default_size: usvg::Size::new(def_size, def_size).unwrap(),
                    fontdb,
                };

                let tree = usvg::Tree::from_data(&data, &opts).unwrap();
                let size = tree.svg_node().size.to_screen_size().dimensions();
                self.tree = Some(tree);
                let size = Vec2(size.0.cast(), size.1.cast());
                self.min_size = Size::from(size * self.min_size_factor * scale_factor);
                self.ideal_size = Size::from(size * self.ideal_size_factor * scale_factor);
            }
        }
    }

    impl Layout for Svg {
        fn size_rules(&mut self, size_mgr: SizeMgr, axis: AxisInfo) -> SizeRules {
            let margins = self.margins.select(size_mgr);
            if axis.is_horizontal() {
                SizeRules::new(
                    self.min_size.0,
                    self.ideal_size.0,
                    margins.horiz,
                    self.stretch,
                )
            } else {
                SizeRules::new(
                    self.min_size.1,
                    self.ideal_size.1,
                    margins.vert,
                    self.stretch,
                )
            }
        }

        fn set_rect(&mut self, mgr: &mut SetRectMgr, rect: Rect, align: AlignHints) {
            let size = match self.ideal_size.aspect_scale_to(rect.size) {
                Some(size) => {
                    self.core_data_mut().rect = align
                        .complete(Align::Center, Align::Center)
                        .aligned_rect(size, rect);
                    Into::<(u32, u32)>::into(size)
                }
                None => {
                    self.core_data_mut().rect = rect;
                    self.pixmap = None;
                    self.image_id = None;
                    return;
                }
            };

            let pm_size = self.pixmap.as_ref().map(|pm| (pm.width(), pm.height()));
            if pm_size.unwrap_or((0, 0)) != size {
                if let Some(id) = self.image_id {
                    mgr.draw_shared().image_free(id);
                }
                self.pixmap = Pixmap::new(size.0, size.1);
                if let Some(tree) = self.tree.as_ref() {
                    self.image_id = self.pixmap.as_mut().map(|pm| {
                        let (w, h) = (pm.width(), pm.height());

                        // alas, we cannot tell resvg to skip the aspect-ratio-scaling!
                        resvg::render(tree, usvg::FitTo::Height(h), pm.as_mut());

                        let id = mgr.draw_shared().image_alloc((w, h)).unwrap();
                        mgr.draw_shared().image_upload(id, pm.data(), ImageFormat::Rgba8);
                        id
                    });
                }
            }
        }

        fn draw(&mut self, mut draw: DrawMgr) {
            let mut draw = draw.with_core(self.core_data());
            if let Some(id) = self.image_id {
                draw.image(id, self.rect());
            }
        }
    }
}
