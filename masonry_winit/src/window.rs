// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use std::hash::Hash;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::field::DisplayValue;

use std::fmt::Debug;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use accesskit_winit::Adapter;
use masonry_core::app::{RenderRoot, RenderRootOptions, RenderRootSignal, WindowSizePolicy};
use masonry_core::core::{DefaultProperties, NewWidget, Widget};
use masonry_core::peniko::Color;
use ui_events_winit::WindowEventReducer;
use winit::dpi::PhysicalSize;
use winit::window::{Window as WindowHandle, WindowAttributes};

/// Per-Window state
pub struct Window {
    pub(crate) id: WindowId,
    pub(crate) handle: Arc<WindowHandle>,
    pub(crate) accesskit_adapter: Adapter,
    pub(crate) event_reducer: WindowEventReducer,
    pub(crate) render_root: RenderRoot,
    pub(crate) base_color: Color,
}

/// A container for a window yet to be created.
///
/// This is stored inside [`MasonryState`] and will be created during the `resumed` event.
///
/// [`MasonryState`]: crate::event_loop_runner::MasonryState
#[derive(Debug)]
pub struct NewWindow {
    /// The id is set by the App, and can be created using the [`WindowId::next()`] method.
    ///
    /// Once the window is created, it can be accessed using this `id` through the
    /// [`DriverCtx::window()`] method.
    ///
    /// [`DriverCtx::window()`]: crate::app_driver::DriverCtx::window
    pub id: WindowId,
    /// Window attributes for the winit's [`Window`].
    ///
    /// A default attribute can be created using [`Window::default_attributes()`].
    ///
    /// [`Window`]: crate::winit::window::Window
    /// [`Window::default_attributes()`]: crate::winit::window::Window::default_attributes()
    pub attributes: WindowAttributes,
    /// The widget which will take up the entire contents of the new window.
    pub root_widget: NewWidget<dyn Widget>,
    /// The base color of the window.
    pub base_color: Color,
}

// TODO - Move to masonry_core?
/// A unique and persistent identifier for a window.
///
/// [`MasonryState`] internally maps these to winit window ids ([`winit::window::WindowId`]).
/// Applications should only use this struct and not be concerned with the winit window ids.
/// When the application is suspended and resumed this id will stay the same, while the
/// winit window id will change.
///
/// [`MasonryState`]: crate::event_loop_runner::MasonryState
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct WindowId(pub(crate) NonZeroU64);

// ---

impl Window {
    pub(crate) fn new(
        window_id: WindowId,
        handle: Arc<WindowHandle>,
        accesskit_adapter: Adapter,
        root_widget: NewWidget<dyn Widget>,
        signal_sender: Sender<(WindowId, RenderRootSignal)>,
        default_properties: Arc<DefaultProperties>,
        base_color: Color,
        size: PhysicalSize<u32>,
        scale_factor: f64,
    ) -> Self {
        Self {
            id: window_id,
            handle,
            accesskit_adapter,
            event_reducer: WindowEventReducer::default(),
            render_root: RenderRoot::new(
                root_widget,
                move |signal| {
                    signal_sender.clone().send((window_id, signal)).unwrap();
                },
                RenderRootOptions {
                    default_properties,
                    use_system_fonts: true,
                    size_policy: WindowSizePolicy::User,
                    size,
                    scale_factor,
                    test_font: None,
                },
            ),
            base_color,
        }
    }

    /// Access the underlying [Winit window](WindowHandle) of this window.
    pub fn handle(&self) -> &winit::window::Window {
        &self.handle
    }

    /// Access the [`RenderRoot`] of this window.
    pub fn render_root(&mut self) -> &mut RenderRoot {
        &mut self.render_root
    }

    /// Access base color of this window.
    pub fn base_color(&mut self) -> &mut Color {
        &mut self.base_color
    }
}

impl Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("id", &self.id)
            .field("handle", &self.handle)
            .field("event_reducer", &self.event_reducer)
            .field("render_root", &self.render_root)
            .field("base_color", &self.base_color)
            .finish_non_exhaustive()
    }
}

impl NewWindow {
    /// Creates a new window with an automatically assigned [`WindowId`].
    ///
    /// See the documentation on the fields of this type for details of the parameters.
    pub fn new(attributes: WindowAttributes, root_widget: NewWidget<dyn Widget + 'static>) -> Self {
        Self::new_with_id(WindowId::next(), attributes, root_widget)
    }

    /// Creates a new window with a custom assigned [`WindowId`].
    ///
    /// Use this when you need to specify a unique ID for the window, for example,
    /// for external tracking or state management.
    ///
    /// Every window you create should have a unique id.
    /// You should never recycle ids from previous windows, even after deleting them.
    pub fn new_with_id(
        id: WindowId,
        attributes: WindowAttributes,
        root_widget: NewWidget<dyn Widget + 'static>,
    ) -> Self {
        Self {
            id,
            attributes,
            root_widget,
            base_color: Color::BLACK,
        }
    }

    /// Sets the base color of the new window.
    ///
    /// The base color is the color of the background which all widgets in the window draw on top of.
    /// Masonry's current default theme assumes that this will be a very dark color for sufficient contrast.
    /// This is most useful for apps which want to for example support light mode.
    ///
    /// Please note that it is not currently supported to modify this once the app is running.
    /// This is not a fundamental limitation, and is only due to missing api design.
    pub fn with_base_color(mut self, base_color: Color) -> Self {
        self.base_color = base_color;
        self
    }
}

impl WindowId {
    /// Allocate a new, unique `WindowId`.
    pub fn next() -> Self {
        static WINDOW_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
        let id = WINDOW_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(id.try_into().unwrap())
    }

    /// A serialized representation of the `WindowId` for debugging purposes.
    pub fn trace(self) -> DisplayValue<NonZeroU64> {
        tracing::field::display(self.0)
    }
}
