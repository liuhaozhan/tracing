//! Dispatches trace events to [`Subscriber`]s.
//!
//! The _dispatcher_ is the component of the tracing system which is responsible
//! for forwarding trace data from the instrumentation points that generate it
//! to the subscriber that collects it.
//!
//! # Using the Trace Dispatcher
//!
//! Every thread in a program using `tracing` has a _default subscriber_. When
//! events occur, or spans are created, they are dispatched to the thread's
//! current subscriber.
//!
//! ## Setting the Default Subscriber
//!
//! By default, the current subscriber is an empty implementation that does
//! nothing. To use a subscriber implementation, it must be set as the default.
//! There are two methods for doing so: [`with_default`] and
//! [`set_global_default`]. `with_default` sets the default subscriber for the
//! duration of a scope, while `set_global_default` sets a default subscriber
//! for the entire process.
//!
//! To use either of these functions, we must first wrap our subscriber in a
//! [`Dispatch`], a cloneable, type-erased reference to a subscriber. For
//! example:
//! ```rust
//! # pub struct FooSubscriber;
//! # use tracing_core::{
//! #   dispatcher, Event, Metadata,
//! #   span::{Attributes, Id, Record}
//! # };
//! # impl tracing_core::Subscriber for FooSubscriber {
//! #   fn new_span(&self, _: &Attributes) -> Id { Id::from_u64(0) }
//! #   fn record(&self, _: &Id, _: &Record) {}
//! #   fn event(&self, _: &Event) {}
//! #   fn record_follows_from(&self, _: &Id, _: &Id) {}
//! #   fn enabled(&self, _: &Metadata) -> bool { false }
//! #   fn enter(&self, _: &Id) {}
//! #   fn exit(&self, _: &Id) {}
//! # }
//! # impl FooSubscriber { fn new() -> Self { FooSubscriber } }
//! # #[cfg(feature = "alloc")]
//! use dispatcher::Dispatch;
//!
//! # #[cfg(feature = "alloc")]
//! let my_subscriber = FooSubscriber::new();
//! # #[cfg(feature = "alloc")]
//! let my_dispatch = Dispatch::new(my_subscriber);
//! ```
//! Then, we can use [`with_default`] to set our `Dispatch` as the default for
//! the duration of a block:
//! ```rust
//! # pub struct FooSubscriber;
//! # use tracing_core::{
//! #   dispatcher, Event, Metadata,
//! #   span::{Attributes, Id, Record}
//! # };
//! # impl tracing_core::Subscriber for FooSubscriber {
//! #   fn new_span(&self, _: &Attributes) -> Id { Id::from_u64(0) }
//! #   fn record(&self, _: &Id, _: &Record) {}
//! #   fn event(&self, _: &Event) {}
//! #   fn record_follows_from(&self, _: &Id, _: &Id) {}
//! #   fn enabled(&self, _: &Metadata) -> bool { false }
//! #   fn enter(&self, _: &Id) {}
//! #   fn exit(&self, _: &Id) {}
//! # }
//! # impl FooSubscriber { fn new() -> Self { FooSubscriber } }
//! # let _my_subscriber = FooSubscriber::new();
//! # #[cfg(feature = "std")]
//! # let my_dispatch = dispatcher::Dispatch::new(_my_subscriber);
//! // no default subscriber
//!
//! # #[cfg(feature = "std")]
//! dispatcher::with_default(&my_dispatch, || {
//!     // my_subscriber is the default
//! });
//!
//! // no default subscriber again
//! ```
//! It's important to note that `with_default` will not propagate the current
//! thread's default subscriber to any threads spawned within the `with_default`
//! block. To propagate the default subscriber to new threads, either use
//! `with_default` from the new thread, or use `set_global_default`.
//!
//! As an alternative to `with_default`, we can use [`set_global_default`] to
//! set a `Dispatch` as the default for all threads, for the lifetime of the
//! program. For example:
//! ```rust
//! # pub struct FooSubscriber;
//! # use tracing_core::{
//! #   dispatcher, Event, Metadata,
//! #   span::{Attributes, Id, Record}
//! # };
//! # impl tracing_core::Subscriber for FooSubscriber {
//! #   fn new_span(&self, _: &Attributes) -> Id { Id::from_u64(0) }
//! #   fn record(&self, _: &Id, _: &Record) {}
//! #   fn event(&self, _: &Event) {}
//! #   fn record_follows_from(&self, _: &Id, _: &Id) {}
//! #   fn enabled(&self, _: &Metadata) -> bool { false }
//! #   fn enter(&self, _: &Id) {}
//! #   fn exit(&self, _: &Id) {}
//! # }
//! # impl FooSubscriber { fn new() -> Self { FooSubscriber } }
//! # #[cfg(feature = "std")]
//! # let my_subscriber = FooSubscriber::new();
//! # #[cfg(feature = "std")]
//! # let my_dispatch = dispatcher::Dispatch::new(my_subscriber);
//! // no default subscriber
//!
//! # #[cfg(feature = "std")]
//! dispatcher::set_global_default(my_dispatch)
//!     // `set_global_default` will return an error if the global default
//!     // subscriber has already been set.
//!     .expect("global default was already set!");
//!
//! // `my_subscriber` is now the default
//! ```
//!
//! <div class="information">
//!     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
//! </div>
//! <div class="example-wrap" style="display:inline-block">
//! <pre class="ignore" style="white-space:normal;font:inherit;">
//!
//! **Note**: the thread-local scoped dispatcher ([`with_default`]) requires the
//! Rust standard library. `no_std` users should use [`set_global_default`] instead.
//!
//! </pre></div>
//!
//! ## Accessing the Default Subscriber
//!
//! A thread's current default subscriber can be accessed using the
//! [`get_default`] function, which executes a closure with a reference to the
//! currently default `Dispatch`. This is used primarily by `tracing`
//! instrumentation.
use crate::{
    span,
    subscriber::{self, Subscriber},
    Event, LevelFilter, Metadata,
};

use core::{
    any::Any,
    fmt,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[cfg(feature = "std")]
use std::{
    cell::{Cell, RefCell, RefMut},
    error,
    sync::Weak,
};

#[cfg(feature = "alloc")]
use alloc::sync::Arc;

#[cfg(feature = "alloc")]
use core::ops::Deref;

/// `Dispatch` trace data to a [`Subscriber`].
#[derive(Clone)]
pub struct Dispatch {
    #[cfg(feature = "alloc")]
    subscriber: Kind<Arc<dyn Subscriber + Send + Sync>>,

    #[cfg(not(feature = "alloc"))]
    subscriber: &'static (dyn Subscriber + Send + Sync),
}

#[cfg(feature = "alloc")]
#[derive(Clone)]
enum Kind<T> {
    Global(&'static (dyn Subscriber + Send + Sync)),
    Scoped(T),
}

#[cfg(feature = "std")]
thread_local! {
    static CURRENT_STATE: State = State {
        default: RefCell::new(Dispatch::none()),
        can_enter: Cell::new(true),
    };
}

static EXISTS: AtomicBool = AtomicBool::new(false);
static GLOBAL_INIT: AtomicUsize = AtomicUsize::new(UNINITIALIZED);

#[cfg(feature = "std")]
static SCOPED_COUNT: AtomicUsize = AtomicUsize::new(0);

const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

static mut GLOBAL_DISPATCH: Dispatch = Dispatch {
    #[cfg(feature = "alloc")]
    subscriber: Kind::Global(&NO_SUBSCRIBER),
    #[cfg(not(feature = "alloc"))]
    subscriber: &NO_SUBSCRIBER,
};
static NONE: Dispatch = Dispatch {
    #[cfg(feature = "alloc")]
    subscriber: Kind::Global(&NO_SUBSCRIBER),
    #[cfg(not(feature = "alloc"))]
    subscriber: &NO_SUBSCRIBER,
};
static NO_SUBSCRIBER: NoSubscriber = NoSubscriber;

/// The dispatch state of a thread.
#[cfg(feature = "std")]
struct State {
    /// This thread's current default dispatcher.
    default: RefCell<Dispatch>,
    /// Whether or not we can currently begin dispatching a trace event.
    ///
    /// This is set to `false` when functions such as `enter`, `exit`, `event`,
    /// and `new_span` are called on this thread's default dispatcher, to
    /// prevent further trace events triggered inside those functions from
    /// creating an infinite recursion. When we finish handling a dispatch, this
    /// is set back to `true`.
    can_enter: Cell<bool>,
}

/// While this guard is active, additional calls to subscriber functions on
/// the default dispatcher will not be able to access the dispatch context.
/// Dropping the guard will allow the dispatch context to be re-entered.
#[cfg(feature = "std")]
struct Entered<'a>(&'a State);

/// A guard that resets the current default dispatcher to the prior
/// default dispatcher when dropped.
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[derive(Debug)]
pub struct DefaultGuard(Option<Dispatch>);

/// Sets this dispatch as the default for the duration of a closure.
///
/// The default dispatcher is used when creating a new [span] or
/// [`Event`].
///
/// <div class="information">
///     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
/// </div>
/// <div class="example-wrap" style="display:inline-block">
/// <pre class="ignore" style="white-space:normal;font:inherit;">
/// <strong>Note</strong>: This function required the Rust standard library.
/// <!-- hack: this whitespace makes rustdoc interpret the next line as markdown again -->
///
/// `no_std` users should use [`set_global_default`] instead.
///
/// </pre></div>
///
/// [span]: super::span
/// [`Subscriber`]: super::subscriber::Subscriber
/// [`Event`]: super::event::Event
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub fn with_default<T>(dispatcher: &Dispatch, f: impl FnOnce() -> T) -> T {
    // When this guard is dropped, the default dispatcher will be reset to the
    // prior default. Using this (rather than simply resetting after calling
    // `f`) ensures that we always reset to the prior dispatcher even if `f`
    // panics.
    let _guard = set_default(dispatcher);
    f()
}

/// Sets the dispatch as the default dispatch for the duration of the lifetime
/// of the returned DefaultGuard
///
/// <div class="information">
///     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
/// </div>
/// <div class="example-wrap" style="display:inline-block">
/// <pre class="ignore" style="white-space:normal;font:inherit;">
///
/// **Note**: This function required the Rust standard library.
/// `no_std` users should use [`set_global_default`] instead.
///
/// </pre></div>
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[must_use = "Dropping the guard unregisters the dispatcher."]
pub fn set_default(dispatcher: &Dispatch) -> DefaultGuard {
    // When this guard is dropped, the default dispatcher will be reset to the
    // prior default. Using this ensures that we always reset to the prior
    // dispatcher even if the thread calling this function panics.
    State::set_default(dispatcher.clone())
}

/// Sets this dispatch as the global default for the duration of the entire program.
/// Will be used as a fallback if no thread-local dispatch has been set in a thread
/// (using `with_default`.)
///
/// Can only be set once; subsequent attempts to set the global default will fail.
/// Returns `Err` if the global default has already been set.
///
///
/// <div class="information">
///     <div class="tooltip compile_fail" style="">&#x26a0; &#xfe0f;<span class="tooltiptext">Warning</span></div>
/// </div><div class="example-wrap" style="display:inline-block"><pre class="compile_fail" style="white-space:normal;font:inherit;">
/// <strong>Warning</strong>: In general, libraries should <em>not</em> call
/// <code>set_global_default()</code>! Doing so will cause conflicts when
/// executables that depend on the library try to set the default later.
/// </pre></div>
///
/// [span]: super::span
/// [`Subscriber`]: super::subscriber::Subscriber
/// [`Event`]: super::event::Event
pub fn set_global_default(dispatcher: Dispatch) -> Result<(), SetGlobalDefaultError> {
    if GLOBAL_INIT.compare_and_swap(UNINITIALIZED, INITIALIZING, Ordering::SeqCst) == UNINITIALIZED
    {
        #[cfg(feature = "alloc")]
        let subscriber = {
            let subscriber = match dispatcher.subscriber {
                Kind::Global(s) => s,
                Kind::Scoped(s) => unsafe {
                    // safety: this leaks the subscriber onto the heap. the
                    // reference count will always be at least 1.
                    &*Arc::into_raw(s)
                },
            };
            Kind::Global(subscriber)
        };

        #[cfg(not(feature = "alloc"))]
        let subscriber = dispatcher.subscriber;

        unsafe {
            GLOBAL_DISPATCH = Dispatch { subscriber };
        }
        GLOBAL_INIT.store(INITIALIZED, Ordering::SeqCst);
        EXISTS.store(true, Ordering::Release);
        Ok(())
    } else {
        Err(SetGlobalDefaultError { _no_construct: () })
    }
}

/// Returns true if a `tracing` dispatcher has ever been set.
///
/// This may be used to completely elide trace points if tracing is not in use
/// at all or has yet to be initialized.
#[doc(hidden)]
#[inline(always)]
pub fn has_been_set() -> bool {
    EXISTS.load(Ordering::Relaxed)
}

/// Returned if setting the global dispatcher fails.
#[derive(Debug)]
pub struct SetGlobalDefaultError {
    _no_construct: (),
}

impl fmt::Display for SetGlobalDefaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("a global default trace dispatcher has already been set")
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl error::Error for SetGlobalDefaultError {}

/// Executes a closure with a reference to this thread's current [dispatcher].
///
/// Note that calls to `get_default` should not be nested; if this function is
/// called while inside of another `get_default`, that closure will be provided
/// with `Dispatch::none` rather than the previously set dispatcher.
///
/// [dispatcher]: super::dispatcher::Dispatch
#[cfg(feature = "std")]
pub fn get_default<T, F>(mut f: F) -> T
where
    F: FnMut(&Dispatch) -> T,
{
    if SCOPED_COUNT.load(Ordering::Acquire) == 0 {
        // fast path if no scoped dispatcher has been set; just use the global
        // default.
        return f(get_global());
    }

    // While this guard is active, additional calls to subscriber functions on
    // the default dispatcher will not be able to access the dispatch context.
    // Dropping the guard will allow the dispatch context to be re-entered.
    struct Entered<'a>(&'a Cell<bool>);
    impl<'a> Drop for Entered<'a> {
        #[inline]
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    CURRENT_STATE
        .try_with(|state| {
            if state.can_enter.replace(false) {
                let _guard = Entered(&state.can_enter);

                let mut default = state.default.borrow_mut();

                if default.is::<NoSubscriber>() {
                    // don't redo this call on the next check
                    *default = get_global().clone();
                }
                return f(&*default);
            }

            f(&Dispatch::none())
        })
        .unwrap_or_else(|_| f(&Dispatch::none()))
}

/// Executes a closure with a reference to this thread's current [dispatcher].
///
/// Note that calls to `get_default` should not be nested; if this function is
/// called while inside of another `get_default`, that closure will be provided
/// with `Dispatch::none` rather than the previously set dispatcher.
///
/// [dispatcher]: super::dispatcher::Dispatch
#[cfg(feature = "std")]
#[doc(hidden)]
#[inline(never)]
pub fn get_current<T>(f: impl FnOnce(&Dispatch) -> T) -> Option<T> {
    CURRENT_STATE
        .try_with(|state| {
            let entered = state.enter()?;
            Some(f(&*entered.current()))
        })
        .ok()?
}

/// Executes a closure with a reference to the current [dispatcher].
///
/// [dispatcher]: super::dispatcher::Dispatch
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub fn get_current<T>(f: impl FnOnce(&Dispatch) -> T) -> Option<T> {
    Some(f(&get_global()))
}

/// Executes a closure with a reference to the current [dispatcher].
///
/// [dispatcher]: super::dispatcher::Dispatch
#[cfg(not(feature = "std"))]
pub fn get_default<T, F>(mut f: F) -> T
where
    F: FnMut(&Dispatch) -> T,
{
    f(get_global())
}

#[inline(always)]
pub(crate) fn get_global() -> &'static Dispatch {
    if GLOBAL_INIT.load(Ordering::Acquire) != INITIALIZED {
        return &NONE;
    }
    unsafe {
        // This is safe given the invariant that setting the global dispatcher
        // also sets `GLOBAL_INIT` to `INITIALIZED`.
        &GLOBAL_DISPATCH
    }
}

#[cfg(feature = "std")]
pub(crate) struct Registrar(Kind<Weak<dyn Subscriber + Send + Sync>>);

impl Dispatch {
    /// Returns a new `Dispatch` that discards events and spans.
    #[inline]
    pub fn none() -> Self {
        Dispatch {
            #[cfg(feature = "alloc")]
            subscriber: Kind::Global(&NO_SUBSCRIBER),
            #[cfg(not(feature = "alloc"))]
            subscriber: &NO_SUBSCRIBER,
        }
    }

    /// Returns a `Dispatch` that forwards to the given [`Subscriber`].
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
    pub fn new<S>(subscriber: S) -> Self
    where
        S: Subscriber + Send + Sync + 'static,
    {
        let me = Dispatch {
            subscriber: Kind::Scoped(Arc::new(subscriber)),
        };
        crate::callsite::register_dispatch(&me);
        me
    }

    /// Returns a `Dispatch` that forwards to the given static [`Subscriber`].
    ///
    /// Unlike [`Dispatch::new`], this function is always available on all
    /// platforms, even when the `std` or `alloc` features are disabled.
    ///
    /// In order to use `from_static`, the `Subscriber` itself must be stored in
    /// a static. For example:
    ///
    /// ```rust
    /// struct MySubscriber {
    ///    // ...
    /// }
    ///
    /// # use tracing_core::{span::{Id, Attributes, Record}, Event, Metadata};
    /// impl tracing_core::Subscriber for MySubscriber {
    ///     // ...
    /// #   fn new_span(&self, _: &Attributes) -> Id { Id::from_u64(0) }
    /// #   fn record(&self, _: &Id, _: &Record) {}
    /// #   fn event(&self, _: &Event) {}
    /// #   fn record_follows_from(&self, _: &Id, _: &Id) {}
    /// #   fn enabled(&self, _: &Metadata) -> bool { false }
    /// #   fn enter(&self, _: &Id) {}
    /// #   fn exit(&self, _: &Id) {}
    /// }
    ///
    /// static SUBSCRIBER: MySubscriber = MySubscriber {
    ///     // ...
    /// };
    ///
    /// fn main() {
    ///     use tracing_core::dispatcher::{self, Dispatch};
    ///
    ///     let dispatch = Dispatch::from_static(&SUBSCRIBER);
    ///
    ///     dispatcher::set_global_default(dispatch)
    ///         .expect("no global default subscriber should have been set previously!");
    /// }
    /// ```
    ///
    /// Constructing the subscriber in a static initializer may make some forms
    /// of runtime configuration more challenging. If this is the case, users
    /// with access to `liballoc` or the Rust standard library are encouraged to
    /// use [`Dispatch::new`] rather than `from_static`. `no_std` users who
    /// cannot allocate or do not have access to `liballoc` may want to consider
    /// the [`lazy_static`] crate, or another library which allows lazy
    /// initialization of statics.
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`Dispatch::new`]: Dispatch::new
    /// [`lazy_static`]: https://crates.io/crates/lazy_static
    pub fn from_static(subscriber: &'static (dyn Subscriber + Send + Sync)) -> Self {
        #[cfg(feature = "alloc")]
        let me = Self {
            subscriber: Kind::Global(subscriber),
        };
        #[cfg(not(feature = "alloc"))]
        let me = Self { subscriber };
        crate::callsite::register_dispatch(&me);
        me
    }

    #[cfg(feature = "std")]
    pub(crate) fn registrar(&self) -> Registrar {
        Registrar(match self.subscriber {
            Kind::Scoped(ref s) => Kind::Scoped(Arc::downgrade(s)),
            Kind::Global(s) => Kind::Global(s),
        })
    }

    #[inline(always)]
    #[cfg(feature = "alloc")]
    fn subscriber(&self) -> &(dyn Subscriber + Send + Sync) {
        match self.subscriber {
            Kind::Scoped(ref s) => Arc::deref(s),
            Kind::Global(s) => s,
        }
    }

    #[inline(always)]
    #[cfg(not(feature = "alloc"))]
    fn subscriber(&self) -> &(dyn Subscriber + Send + Sync) {
        self.subscriber
    }

    /// Registers a new callsite with this subscriber, returning whether or not
    /// the subscriber is interested in being notified about the callsite.
    ///
    /// This calls the [`register_callsite`] function on the [`Subscriber`]
    /// that this `Dispatch` forwards to.
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`register_callsite`]: super::subscriber::Subscriber::register_callsite
    #[inline]
    pub fn register_callsite(&self, metadata: &'static Metadata<'static>) -> subscriber::Interest {
        self.subscriber().register_callsite(metadata)
    }

    /// Returns the highest [verbosity level][level] that this [`Subscriber`] will
    /// enable, or `None`, if the subscriber does not implement level-based
    /// filtering or chooses not to implement this method.
    ///
    /// This calls the [`max_level_hint`] function on the [`Subscriber`]
    /// that this `Dispatch` forwards to.
    ///
    /// [level]: super::Level
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`register_callsite`]: super::subscriber::Subscriber::max_level_hint
    // TODO(eliza): consider making this a public API?
    #[inline]
    pub(crate) fn max_level_hint(&self) -> Option<LevelFilter> {
        self.subscriber().max_level_hint()
    }

    /// Record the construction of a new span, returning a new [ID] for the
    /// span being constructed.
    ///
    /// This calls the [`new_span`] function on the [`Subscriber`] that this
    /// `Dispatch` forwards to.
    ///
    /// [ID]: super::span::Id
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`new_span`]: super::subscriber::Subscriber::new_span
    #[inline]
    pub fn new_span(&self, span: &span::Attributes<'_>) -> span::Id {
        self.subscriber().new_span(span)
    }

    /// Record a set of values on a span.
    ///
    /// This calls the [`record`] function on the [`Subscriber`] that this
    /// `Dispatch` forwards to.
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`record`]: super::subscriber::Subscriber::record
    #[inline]
    pub fn record(&self, span: &span::Id, values: &span::Record<'_>) {
        self.subscriber().record(span, values)
    }

    /// Adds an indication that `span` follows from the span with the id
    /// `follows`.
    ///
    /// This calls the [`record_follows_from`] function on the [`Subscriber`]
    /// that this `Dispatch` forwards to.
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`record_follows_from`]: super::subscriber::Subscriber::record_follows_from
    #[inline]
    pub fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
        self.subscriber().record_follows_from(span, follows)
    }

    /// Returns true if a span with the specified [metadata] would be
    /// recorded.
    ///
    /// This calls the [`enabled`] function on the [`Subscriber`] that this
    /// `Dispatch` forwards to.
    ///
    /// [metadata]: super::metadata::Metadata
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`enabled`]: super::subscriber::Subscriber::enabled
    #[inline]
    pub fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.subscriber().enabled(metadata)
    }

    /// Records that an [`Event`] has occurred.
    ///
    /// This calls the [`event`] function on the [`Subscriber`] that this
    /// `Dispatch` forwards to.
    ///
    /// [`Event`]: super::event::Event
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`event`]: super::subscriber::Subscriber::event
    #[inline]
    pub fn event(&self, event: &Event<'_>) {
        self.subscriber().event(event)
    }

    /// Records that a span has been can_enter.
    ///
    /// This calls the [`enter`] function on the [`Subscriber`] that this
    /// `Dispatch` forwards to.
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`enter`]: super::subscriber::Subscriber::enter
    #[inline]
    pub fn enter(&self, span: &span::Id) {
        self.subscriber().enter(span);
    }

    /// Records that a span has been exited.
    ///
    /// This calls the [`exit`] function on the [`Subscriber`] that this
    /// `Dispatch` forwards to.
    ///
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`exit`]: super::subscriber::Subscriber::exit
    #[inline]
    pub fn exit(&self, span: &span::Id) {
        self.subscriber().exit(span);
    }

    /// Notifies the subscriber that a [span ID] has been cloned.
    ///
    /// This function must only be called with span IDs that were returned by
    /// this `Dispatch`'s [`new_span`] function. The `tracing` crate upholds
    /// this guarantee and any other libraries implementing instrumentation APIs
    /// must as well.
    ///
    /// This calls the [`clone_span`] function on the `Subscriber` that this
    /// `Dispatch` forwards to.
    ///
    /// [span ID]: super::span::Id
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`clone_span`]: super::subscriber::Subscriber::clone_span
    /// [`new_span`]: super::subscriber::Subscriber::new_span
    #[inline]
    pub fn clone_span(&self, id: &span::Id) -> span::Id {
        self.subscriber().clone_span(&id)
    }

    /// Notifies the subscriber that a [span ID] has been dropped.
    ///
    /// This function must only be called with span IDs that were returned by
    /// this `Dispatch`'s [`new_span`] function. The `tracing` crate upholds
    /// this guarantee and any other libraries implementing instrumentation APIs
    /// must as well.
    ///
    /// This calls the [`drop_span`] function on the [`Subscriber`] that this
    ///  `Dispatch` forwards to.
    ///
    /// <div class="information">
    ///     <div class="tooltip compile_fail" style="">&#x26a0; &#xfe0f;<span class="tooltiptext">Warning</span></div>
    /// </div>
    /// <div class="example-wrap" style="display:inline-block"><pre class="compile_fail" style="white-space:normal;font:inherit;">
    ///
    /// **Deprecated**: The [`try_close`] method is functionally identical, but returns `true` if the span is now closed.
    /// It should be used instead of this method.
    ///
    /// </pre></div>
    ///
    /// [span ID]: super::span::Id
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`drop_span`]: super::subscriber::Subscriber::drop_span
    /// [`new_span`]: super::subscriber::Subscriber::new_span
    /// [`try_close`]: Self::try_close
    #[inline]
    #[deprecated(since = "0.1.2", note = "use `Dispatch::try_close` instead")]
    pub fn drop_span(&self, id: span::Id) {
        #[allow(deprecated)]
        self.subscriber().drop_span(id);
    }

    /// Notifies the subscriber that a [span ID] has been dropped, and returns
    /// `true` if there are now 0 IDs referring to that span.
    ///
    /// This function must only be called with span IDs that were returned by
    /// this `Dispatch`'s [`new_span`] function. The `tracing` crate upholds
    /// this guarantee and any other libraries implementing instrumentation APIs
    /// must as well.
    ///
    /// This calls the [`try_close`] function on the [`Subscriber`] that this
    ///  `Dispatch` forwards to.
    ///
    /// [span ID]: super::span::Id
    /// [`Subscriber`]: super::subscriber::Subscriber
    /// [`try_close`]: super::subscriber::Subscriber::try_close
    /// [`new_span`]: super::subscriber::Subscriber::new_span
    #[inline]
    pub fn try_close(&self, id: span::Id) -> bool {
        self.subscriber().try_close(id)
    }

    /// Returns a type representing this subscriber's view of the current span.
    ///
    /// This calls the [`current`] function on the `Subscriber` that this
    /// `Dispatch` forwards to.
    ///
    /// [`current`]: super::subscriber::Subscriber::current_span
    #[inline]
    pub fn current_span(&self) -> span::Current {
        self.subscriber().current_span()
    }

    /// Returns `true` if this `Dispatch` forwards to a `Subscriber` of type
    /// `T`.
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        Subscriber::is::<T>(&*self.subscriber())
    }

    /// Returns some reference to the `Subscriber` this `Dispatch` forwards to
    /// if it is of type `T`, or `None` if it isn't.
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        Subscriber::downcast_ref(&*self.subscriber())
    }
}

impl Default for Dispatch {
    /// Returns the current default dispatcher
    fn default() -> Self {
        get_default(|default| default.clone())
    }
}

impl fmt::Debug for Dispatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Dispatch(...)")
    }
}

#[cfg(feature = "std")]
impl<S> From<S> for Dispatch
where
    S: Subscriber + Send + Sync + 'static,
{
    #[inline]
    fn from(subscriber: S) -> Self {
        Dispatch::new(subscriber)
    }
}

struct NoSubscriber;
impl Subscriber for NoSubscriber {
    #[inline]
    fn register_callsite(&self, _: &'static Metadata<'static>) -> subscriber::Interest {
        subscriber::Interest::never()
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(0xDEAD)
    }

    fn event(&self, _event: &Event<'_>) {}

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    #[inline]
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        false
    }

    fn enter(&self, _span: &span::Id) {}
    fn exit(&self, _span: &span::Id) {}
}

#[cfg(feature = "std")]
impl Registrar {
    pub(crate) fn upgrade(&self) -> Option<Dispatch> {
        match self.0 {
            Kind::Global(s) => Some(Dispatch {
                subscriber: Kind::Global(s),
            }),
            Kind::Scoped(ref s) => s.upgrade().map(|s| Dispatch {
                subscriber: Kind::Scoped(s),
            }),
        }
    }
}

// ===== impl State =====

#[cfg(feature = "std")]
impl State {
    /// Replaces the current default dispatcher on this thread with the provided
    /// dispatcher.Any
    ///
    /// Dropping the returned `ResetGuard` will reset the default dispatcher to
    /// the previous value.
    #[inline]
    fn set_default(new_dispatch: Dispatch) -> DefaultGuard {
        let prior = CURRENT_STATE
            .try_with(|state| {
                state.can_enter.set(true);
                state.default.replace(new_dispatch)
            })
            .ok();
        EXISTS.store(true, Ordering::Release);
        SCOPED_COUNT.fetch_add(1, Ordering::Release);
        DefaultGuard(prior)
    }

    #[inline]
    fn enter(&self) -> Option<Entered<'_>> {
        if self.can_enter.replace(false) {
            Some(Entered(&self))
        } else {
            None
        }
    }
}

// ===== impl Entered =====

#[cfg(feature = "std")]
impl<'a> Entered<'a> {
    #[inline]
    fn current(&self) -> RefMut<'a, Dispatch> {
        let mut default = self.0.default.borrow_mut();

        if default.is::<NoSubscriber>() {
            // don't redo this call on the next check
            *default = get_global().clone();
        }

        default
    }
}

#[cfg(feature = "std")]
impl<'a> Drop for Entered<'a> {
    #[inline]
    fn drop(&mut self) {
        self.0.can_enter.set(true);
    }
}

// ===== impl DefaultGuard =====

#[cfg(feature = "std")]
impl Drop for DefaultGuard {
    #[inline]
    fn drop(&mut self) {
        SCOPED_COUNT.fetch_sub(1, Ordering::Release);
        if let Some(dispatch) = self.0.take() {
            // Replace the dispatcher and then drop the old one outside
            // of the thread-local context. Dropping the dispatch may
            // lead to the drop of a subscriber which, in the process,
            // could then also attempt to access the same thread local
            // state -- causing a clash.
            let prev = CURRENT_STATE.try_with(|state| state.default.replace(dispatch));
            drop(prev)
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::{
        callsite::Callsite,
        metadata::{Kind, Level, Metadata},
        subscriber::Interest,
    };

    #[test]
    fn dispatch_is() {
        let dispatcher = Dispatch::from_static(&NO_SUBSCRIBER);
        assert!(dispatcher.is::<NoSubscriber>());
    }

    #[test]
    fn dispatch_downcasts() {
        let dispatcher = Dispatch::from_static(&NO_SUBSCRIBER);
        assert!(dispatcher.downcast_ref::<NoSubscriber>().is_some());
    }

    struct TestCallsite;
    static TEST_CALLSITE: TestCallsite = TestCallsite;
    static TEST_META: Metadata<'static> = metadata! {
        name: "test",
        target: module_path!(),
        level: Level::DEBUG,
        fields: &[],
        callsite: &TEST_CALLSITE,
        kind: Kind::EVENT
    };

    impl Callsite for TestCallsite {
        fn set_interest(&self, _: Interest) {}
        fn metadata(&self) -> &Metadata<'_> {
            &TEST_META
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn events_dont_infinite_loop() {
        // This test ensures that an event triggered within a subscriber
        // won't cause an infinite loop of events.
        struct TestSubscriber;
        impl Subscriber for TestSubscriber {
            fn enabled(&self, _: &Metadata<'_>) -> bool {
                true
            }

            fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
                span::Id::from_u64(0xAAAA)
            }

            fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

            fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

            fn event(&self, _: &Event<'_>) {
                static EVENTS: AtomicUsize = AtomicUsize::new(0);
                assert_eq!(
                    EVENTS.fetch_add(1, Ordering::Relaxed),
                    0,
                    "event method called twice!"
                );
                Event::dispatch(&TEST_META, &TEST_META.fields().value_set(&[]))
            }

            fn enter(&self, _: &span::Id) {}

            fn exit(&self, _: &span::Id) {}
        }

        with_default(&Dispatch::new(TestSubscriber), || {
            Event::dispatch(&TEST_META, &TEST_META.fields().value_set(&[]))
        })
    }

    #[test]
    #[cfg(feature = "std")]
    fn spans_dont_infinite_loop() {
        // This test ensures that a span created within a subscriber
        // won't cause an infinite loop of new spans.

        fn mk_span() {
            get_default(|current| {
                current.new_span(&span::Attributes::new(
                    &TEST_META,
                    &TEST_META.fields().value_set(&[]),
                ))
            });
        }

        struct TestSubscriber;
        impl Subscriber for TestSubscriber {
            fn enabled(&self, _: &Metadata<'_>) -> bool {
                true
            }

            fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
                static NEW_SPANS: AtomicUsize = AtomicUsize::new(0);
                assert_eq!(
                    NEW_SPANS.fetch_add(1, Ordering::Relaxed),
                    0,
                    "new_span method called twice!"
                );
                mk_span();
                span::Id::from_u64(0xAAAA)
            }

            fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

            fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

            fn event(&self, _: &Event<'_>) {}

            fn enter(&self, _: &span::Id) {}

            fn exit(&self, _: &span::Id) {}
        }

        with_default(&Dispatch::new(TestSubscriber), mk_span)
    }

    #[test]
    fn default_no_subscriber() {
        let default_dispatcher = Dispatch::default();
        assert!(default_dispatcher.is::<NoSubscriber>());
    }

    #[cfg(feature = "std")]
    #[test]
    fn default_dispatch() {
        struct TestSubscriber;
        impl Subscriber for TestSubscriber {
            fn enabled(&self, _: &Metadata<'_>) -> bool {
                true
            }

            fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
                span::Id::from_u64(0xAAAA)
            }

            fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

            fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

            fn event(&self, _: &Event<'_>) {}

            fn enter(&self, _: &span::Id) {}

            fn exit(&self, _: &span::Id) {}
        }
        let guard = set_default(&Dispatch::new(TestSubscriber));
        let default_dispatcher = Dispatch::default();
        assert!(default_dispatcher.is::<TestSubscriber>());

        drop(guard);
        let default_dispatcher = Dispatch::default();
        assert!(default_dispatcher.is::<NoSubscriber>());
    }
}
