// Copyright 2025 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::Widget;
use crate::core::{EventCtx, PointerEvent, PropertiesMut};

// TODO - Add NewSubwindow type

/// The trait implemented by widgets which are meant to be at the root of
/// a [subwindow](crate::doc::masonry_concepts#subwindows).
pub trait Subwindow: Widget {
    // TODO - Possible evolutions:
    // - Return flag to suppress event from reaching children.
    // - Return flag to remove subwindow.
    // - Pass subwindow id to method.

    /// An event handler called for every subwindow for all pointer events, even those outside the subwindow's root widget.
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    );
}

/// A unique identifier for a single [`Subwindow`].
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct SubwindowId(pub(crate) NonZeroU64);

/// The type of a new [`Subwindow`].
///
/// When adding a new [subwindow](crate::doc::masonry_concepts#subwindows) to the app,
/// this tells the app driver what type of item to add.
#[derive(Clone, Debug, Default)]
pub enum SubwindowType {
    /// A simple tooltip showing some text until the mouse moves.
    Tooltip(String),
    /// A menu showing the different options of a selector widget.
    Selector {
        /// The text of the options.
        options: Vec<String>,
        /// The initially selected option.
        selected_option: usize,
    },
    /// Unknown subwindow type. Always use the widget fallback.
    #[default]
    Other,
}

// ---

impl SubwindowId {
    /// Allocates a new, unique `SubwindowId`.
    pub fn next() -> Self {
        static SUBWIDGET_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
        let id = SUBWIDGET_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(id.try_into().unwrap())
    }

    /// Returns the integer value of the `SubwindowId`.
    pub fn to_raw(self) -> u64 {
        self.0.into()
    }
}

impl From<SubwindowId> for u64 {
    fn from(id: SubwindowId) -> Self {
        id.0.into()
    }
}

impl From<SubwindowId> for accesskit::NodeId {
    fn from(id: SubwindowId) -> Self {
        Self(id.0.into())
    }
}

impl Display for SubwindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl PartialEq<accesskit::NodeId> for SubwindowId {
    fn eq(&self, other: &accesskit::NodeId) -> bool {
        self.to_raw() == other.0
    }
}

impl PartialEq<SubwindowId> for accesskit::NodeId {
    fn eq(&self, other: &SubwindowId) -> bool {
        self.0 == other.to_raw()
    }
}
