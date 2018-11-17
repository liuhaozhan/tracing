pub use tokio_trace_core::field::*;

use std::fmt;
use {subscriber::RecordError, Meta};

/// Trait implemented to allow a type to be used as a field key.
///
/// **Note**: Although this is implemented for both the [`Key`] type *and* any
/// type that can be borrowed as an `&str`, only `Key` allows _O_(1) access.
/// Indexing a field with a string results in an iterative search that performs
/// string comparisons. Thus, if possible, once the key for a field is known, it
/// should be used whenever possible.
pub trait AsKey {
    /// Attempts to convert `&self` into a `Key` with the specified `metadata`.
    ///
    /// If `metadata` defines a key corresponding to this field, then the key is
    /// returned. Otherwise, this function returns `None`.
    fn as_key<'a>(&self, metadata: &'a Meta<'a>) -> Option<Key<'a>>;
}

pub trait Record {
    /// Record a signed 64-bit integer value.
    ///
    /// This defaults to calling `self.record_fmt()`; implementations wishing to
    /// provide behaviour specific to signed integers may override the default
    /// implementation.
    ///
    /// This is expected to return an error under the following conditions:
    /// - The span ID does not correspond to a span which currently exists.
    /// - The span does not have a field with the given name.
    /// - The span has a field with the given name, but the value has already
    ///   been set.
    fn record_i64<Q: ?Sized>(&mut self, field: &Q, value: i64) -> Result<(), RecordError>
    where
        Q: AsKey;

    /// Record an umsigned 64-bit integer value.
    ///
    /// This defaults to calling `self.record_fmt()`; implementations wishing to
    /// provide behaviour specific to unsigned integers may override the default
    /// implementation.
    ///
    /// This is expected to return an error under the following conditions:
    /// - The span ID does not correspond to a span which currently exists.
    /// - The span does not have a field with the given name.
    /// - The span has a field with the given name, but the value has already
    ///   been set.
    fn record_u64<Q: ?Sized>(&mut self, field: &Q, value: u64) -> Result<(), RecordError>
    where
        Q: AsKey;

    /// Record a boolean value.
    ///
    /// This defaults to calling `self.record_fmt()`; implementations wishing to
    /// provide behaviour specific to booleans may override the default
    /// implementation.
    ///
    /// This is expected to return an error under the following conditions:
    /// - The span ID does not correspond to a span which currently exists.
    /// - The span does not have a field with the given name.
    /// - The span has a field with the given name, but the value has already
    ///   been set.
    fn record_bool<Q: ?Sized>(&mut self, field: &Q, value: bool) -> Result<(), RecordError>
    where
        Q: AsKey;

    /// Record a string value.
    ///
    /// This defaults to calling `self.record_str()`; implementations wishing to
    /// provide behaviour specific to strings may override the default
    /// implementation.
    ///
    /// This is expected to return an error under the following conditions:
    /// - The span ID does not correspond to a span which currently exists.
    /// - The span does not have a field with the given name.
    /// - The span has a field with the given name, but the value has already
    ///   been set.
    fn record_str<Q: ?Sized>(&mut self, field: &Q, value: &str) -> Result<(), RecordError>
    where
        Q: AsKey;

    /// Record a set of pre-compiled format arguments.
    ///
    /// This is expected to return an error under the following conditions:
    /// - The span ID does not correspond to a span which currently exists.
    /// - The span does not have a field with the given name.
    /// - The span has a field with the given name, but the value has already
    ///   been set.
    fn record_fmt<Q: ?Sized>(
        &mut self,
        field: &Q,
        value: fmt::Arguments,
    ) -> Result<(), RecordError>
    where
        Q: AsKey;
}

/// A field value of an erased type.
///
/// Implementors of `Value` may call the appropriate typed recording methods on
/// the `Subscriber` passed to `record` in order to indicate how their data
/// should be recorded.
pub trait Value {
    /// Records this value with the given `Subscriber`.
    fn record<Q: ?Sized, R>(
        &self,
        key: &Q,
        recorder: &mut R,
    ) -> Result<(), ::subscriber::RecordError>
    where
        Q: AsKey,
        R: Record;
}

/// A `Value` which serializes as a string using `fmt::Display`.
#[derive(Clone)]
pub struct DisplayValue<T: fmt::Display>(T);

/// A `Value` which serializes as a string using `fmt::Debug`.
#[derive(Clone)]
pub struct DebugValue<T: fmt::Debug>(T);

/// Wraps a type implementing `fmt::Display` as a `Value` that can be
/// recorded using its `Display` implementation.
pub fn display<'a, T>(t: T) -> DisplayValue<T>
where
    T: fmt::Display,
{
    DisplayValue(t)
}

// ===== impl Value =====

/// Wraps a type implementing `fmt::Debug` as a `Value` that can be
/// recorded using its `Debug` implementation.
pub fn debug<T>(t: T) -> DebugValue<T>
where
    T: fmt::Debug,
{
    DebugValue(t)
}

macro_rules! impl_values {
    ( $( $record:ident( $( $whatever:tt)+ ) ),+ ) => {
        $(
            impl_value!{ $record( $( $whatever )+ ) }
        )+
    }
}
macro_rules! impl_value {
    ( $record:ident( $( $value_ty:ty ),+ ) ) => {
        $(
            impl $crate::field::Value for $value_ty {
                fn record<Q: ?Sized, R>(
                    &self,
                    key: &Q,
                    recorder: &mut R,
                ) -> Result<(), $crate::subscriber::RecordError>
                where
                    Q: $crate::field::AsKey,
                    R: $crate::field::Record,
                {
                    recorder.$record(key, *self)
                }
            }
        )+
    };
    ( $record:ident( $( $value_ty:ty ),+ as $as_ty:ty) ) => {
        $(
            impl Value for $value_ty {
                fn record<Q: ?Sized, R>(
                    &self,
                    key: &Q,
                    recorder: &mut R,
                ) -> Result<(), $crate::subscriber::RecordError>
                where
                    Q: $crate::field::AsKey,
                    R: $crate::field::Record,
                {
                    recorder.$record(key, *self as $as_ty)
                }
            }
        )+
    };
}

// ===== impl AsKey =====

impl<'f> AsKey for Key<'f> {
    #[inline]
    fn as_key<'a>(&self, metadata: &'a Meta<'a>) -> Option<Key<'a>> {
        self.with_metadata(metadata)
    }
}

impl<'f> AsKey for &'f Key<'f> {
    #[inline]
    fn as_key<'a>(&self, metadata: &'a Meta<'a>) -> Option<Key<'a>> {
        self.with_metadata(metadata)
    }
}

impl AsKey for str {
    #[inline]
    fn as_key<'a>(&self, metadata: &'a Meta<'a>) -> Option<Key<'a>> {
        metadata.key_for(&self)
    }
}

// ===== impl Value =====

impl_values! {
    record_u64(u64),
    record_u64(usize, u32, u16 as u64),
    record_i64(i64),
    record_i64(isize, i32, i16, i8 as i64),
    record_bool(bool)
}

impl Value for str {
    fn record<Q: ?Sized, R>(
        &self,
        key: &Q,
        recorder: &mut R,
    ) -> Result<(), ::subscriber::RecordError>
    where
        Q: AsKey,
        R: Record,
    {
        recorder.record_str(key, &self)
    }
}

impl<'a, T: ?Sized> Value for &'a T
where
    T: Value + 'a,
{
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R) -> Result<(), RecordError>
    where
        Q: AsKey,
        R: Record,
    {
        (*self).record(key, recorder)
    }
}

// ===== impl DisplayValue =====

impl<T> Value for DisplayValue<T>
where
    T: fmt::Display,
{
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R) -> Result<(), RecordError>
    where
        Q: AsKey,
        R: Record,
    {
        recorder.record_fmt(key, format_args!("{}", self.0))
    }
}

// ===== impl DebugValue =====

impl<T: fmt::Debug> Value for DebugValue<T>
where
    T: fmt::Debug,
{
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R) -> Result<(), RecordError>
    where
        Q: AsKey,
        R: Record,
    {
        recorder.record_fmt(key, format_args!("{:?}", self.0))
    }
}
