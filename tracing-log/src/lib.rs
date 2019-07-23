//! Adapters for connecting unstructured log records from the `log` crate into
//! the `tracing` ecosystem.
//!
//! ## Convert log records to tracing `Event`s
//!
//! To convert [`log::Record`]s as [`tracing::Event`]s, set `LogTracer` as the default
//! logger by calling its [`init`] or [`init_with_filter`] methods.
//!
//! ```rust
//! # use std::error::Error;
//! use tracing_log::LogTracer;
//! use log;
//!
//! # fn main() -> Result<(), Box<Error>> {
//! LogTracer::init()?;
//!
//! // will be available for Subscribers as a tracing Event
//! log::trace!("an example trace log");
//! # Ok(())
//! # }
//! ```
//!
//! This conversion does not convert unstructured data in log records (such as
//! values passed as format arguments to the `log!` macro) to structured
//! `tracing` fields. However, it *does* attach these new events to to the
//! span that was currently executing when the record was logged. This is the
//! primary use-case for this library: making it possible to locate the log
//! records emitted by dependencies which use `log` within the context of a
//! trace.
//!
//! ## Convert tracing `Event`s to logs
//!
//! This conversion can be done with [`TraceLogger`], a [`Subscriber`] which
//! records `tracing` spans and events and outputs log records.
//!
//! ## Caution: Mixing both conversions
//!
//! Note that logger implementations that convert log records to trace events
//! should not be used with `Subscriber`s that convert trace events _back_ into
//! log records (such as the `TraceLogger`), as doing so will result in the
//! event recursing between the subscriber and the logger forever (or, in real
//! life, probably overflowing the call stack).
//!
//! If the logging of trace events generated from log records produced by the
//! `log` crate is desired, either the `log` crate should not be used to
//! implement this logging, or an additional layer of filtering will be
//! required to avoid infinitely converting between `Event` and `log::Record`.
//!
//! [`init`]: struct.LogTracer.html#method.init
//! [`init_with_filter`]: struct.LogTracer.html#method.init_with_filter
//! [`TraceLogger`]: struct.TraceLogger.html
//! [`tracing::Event`]: https://docs.rs/tracing/0.1.3/tracing/struct.Event.html
//! [`log::Record`]: https://docs.rs/log/0.4.7/log/struct.Record.html
extern crate log;
extern crate tracing_core;
extern crate tracing_subscriber;

use lazy_static::lazy_static;

use std::io;

use tracing_core::{
    callsite::{self, Callsite},
    dispatcher, field, identify_callsite,
    metadata::Kind,
    subscriber, Event, Metadata,
};

pub mod log_tracer;
pub use self::log_tracer::LogTracer;
pub mod trace_logger;
pub use self::trace_logger::{Builder as TraceLoggerBuilder, TraceLogger};

/// Format a log record as a trace event in the current span.
pub fn format_trace(record: &log::Record) -> io::Result<()> {
    let filter_meta = record.as_trace();
    if !dispatcher::get_default(|dispatch| dispatch.enabled(&filter_meta)) {
        return Ok(());
    };

    let (cs, keys) = match record.level() {
        log::Level::Trace => *TRACE_CS,
        log::Level::Debug => *DEBUG_CS,
        log::Level::Info => *INFO_CS,
        log::Level::Warn => *WARN_CS,
        log::Level::Error => *ERROR_CS,
    };

    let log_module = record.module_path();
    let log_file = record.file();
    let log_line = record.line();

    let module = log_module.as_ref().map(|s| s as &dyn field::Value);
    let file = log_file.as_ref().map(|s| s as &dyn field::Value);
    let line = log_line.as_ref().map(|s| s as &dyn field::Value);

    let meta = cs.metadata();
    Event::dispatch(
        &meta,
        &meta.fields().value_set(&[
            (&keys.message, Some(record.args() as &dyn field::Value)),
            (&keys.target, Some(&record.target())),
            (&keys.module, module),
            (&keys.file, file),
            (&keys.line, line),
        ]),
    );
    Ok(())
}

pub trait AsLog {
    type Log;
    fn as_log(&self) -> Self::Log;
}

pub trait AsTrace {
    type Trace;
    fn as_trace(&self) -> Self::Trace;
}

impl<'a> AsLog for Metadata<'a> {
    type Log = log::Metadata<'a>;
    fn as_log(&self) -> Self::Log {
        log::Metadata::builder()
            .level(self.level().as_log())
            .target(self.target())
            .build()
    }
}

struct Fields {
    message: field::Field,
    target: field::Field,
    module: field::Field,
    file: field::Field,
    line: field::Field,
}

static FIELD_NAMES: &'static [&'static str] = &[
    "message",
    "log.target",
    "log.module_path",
    "log.file",
    "log.line",
];

macro_rules! log_cs {
    ($level:expr) => {{
        struct Callsite;
        static META: Metadata = Metadata::new(
            "log event",
            "log",
            $level,
            None,
            None,
            None,
            field::FieldSet::new(FIELD_NAMES, identify_callsite!(&Callsite)),
            Kind::EVENT,
        );

        impl callsite::Callsite for Callsite {
            fn set_interest(&self, _: subscriber::Interest) {}
            fn metadata(&self) -> &'static Metadata<'static> {
                &META
            }
        }

        lazy_static! {
            static ref FIELDS: Fields = {
                let message = META.fields().field("message").unwrap();
                let target = META.fields().field("log.target").unwrap();
                let module = META.fields().field("log.module_path").unwrap();
                let file = META.fields().field("log.file").unwrap();
                let line = META.fields().field("log.line").unwrap();
                Fields {
                    message,
                    target,
                    module,
                    file,
                    line,
                }
            };
        }
        (&Callsite, &FIELDS)
    }};
}

lazy_static! {
    static ref TRACE_CS: (&'static dyn Callsite, &'static Fields) =
        log_cs!(tracing_core::Level::TRACE);
    static ref DEBUG_CS: (&'static dyn Callsite, &'static Fields) =
        log_cs!(tracing_core::Level::DEBUG);
    static ref INFO_CS: (&'static dyn Callsite, &'static Fields) =
        log_cs!(tracing_core::Level::INFO);
    static ref WARN_CS: (&'static dyn Callsite, &'static Fields) =
        log_cs!(tracing_core::Level::WARN);
    static ref ERROR_CS: (&'static dyn Callsite, &'static Fields) =
        log_cs!(tracing_core::Level::ERROR);
}

impl<'a> AsTrace for log::Record<'a> {
    type Trace = Metadata<'a>;
    fn as_trace(&self) -> Self::Trace {
        let cs_id = match self.level() {
            log::Level::Trace => identify_callsite!(TRACE_CS.0),
            log::Level::Debug => identify_callsite!(DEBUG_CS.0),
            log::Level::Info => identify_callsite!(INFO_CS.0),
            log::Level::Warn => identify_callsite!(WARN_CS.0),
            log::Level::Error => identify_callsite!(ERROR_CS.0),
        };
        Metadata::new(
            "log record",
            self.target(),
            self.level().as_trace(),
            self.file(),
            self.line(),
            self.module_path(),
            field::FieldSet::new(FIELD_NAMES, cs_id),
            Kind::EVENT,
        )
    }
}

impl AsLog for tracing_core::Level {
    type Log = log::Level;
    fn as_log(&self) -> log::Level {
        match *self {
            tracing_core::Level::ERROR => log::Level::Error,
            tracing_core::Level::WARN => log::Level::Warn,
            tracing_core::Level::INFO => log::Level::Info,
            tracing_core::Level::DEBUG => log::Level::Debug,
            tracing_core::Level::TRACE => log::Level::Trace,
        }
    }
}

impl AsTrace for log::Level {
    type Trace = tracing_core::Level;
    fn as_trace(&self) -> tracing_core::Level {
        match self {
            log::Level::Error => tracing_core::Level::ERROR,
            log::Level::Warn => tracing_core::Level::WARN,
            log::Level::Info => tracing_core::Level::INFO,
            log::Level::Debug => tracing_core::Level::DEBUG,
            log::Level::Trace => tracing_core::Level::TRACE,
        }
    }
}
