//! [Subscribers](crate::subscribe) that control which spans and events are
//! enabled by the wrapped collector.
//!
//! This module contains a number of types that provide implementations of
//! various strategies for filtering which spans and events are enabled. For
//! details on filtering spans and events using [`Subscribe`] implementations,
//! see the [`subscribe` module documentation].
//!
//! [`subscribe` module documentation]: crate::subscribe#filtering-with-subscribers
//! [`Subscribe`]: crate::subscribe
#[cfg(feature = "env-filter")]
mod env;
mod filter_fn;
mod level;
#[cfg(feature = "registry")]
mod subscriber_filters;

pub use self::filter_fn::*;
#[cfg(not(feature = "registry"))]
pub(crate) use self::has_psf_stubs::*;
#[cfg(feature = "registry")]
#[cfg_attr(docsrs, doc(cfg(feature = "registry")))]
pub use self::subscriber_filters::*;

pub use self::level::{LevelFilter, ParseError as LevelParseError};

#[cfg(feature = "env-filter")]
#[cfg_attr(docsrs, doc(cfg(feature = "env-filter")))]
pub use self::env::*;

/// Stub implementations of the per-subscriber-fitler detection functions for
/// when the `registry` feature is disabled.
#[cfg(not(feature = "registry"))]
mod has_psf_stubs {
    pub(crate) fn is_psf_downcast_marker(_: std::any::TypeId) -> bool {
        false
    }

    /// Does a type implementing `Collect` contain any per-subscriber filters?
    pub(crate) fn collector_has_psf<C>(_: &C) -> bool
    where
        C: tracing_core::Collect,
    {
        false
    }

    /// Does a type implementing `Subscribe` contain any per-subscriber filters?
    pub(crate) fn subscriber_has_psf<S, C>(_: &S) -> bool
    where
        S: crate::Subscribe<C>,
        C: tracing_core::Collect,
    {
        false
    }
}
