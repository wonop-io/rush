//! Rush Output - Terminal output and logging

pub mod buffered;
pub mod cli;
pub mod config;
pub mod director;
pub mod event;
pub mod example;
pub mod factory;
pub mod file;
pub mod filter;
pub mod formatter;
pub mod router;
pub mod session;
pub mod shared;
pub mod sink;
pub mod source;
pub mod stream;

pub use buffered::BufferedOutputDirector;
pub use director::{OutputDirector, StdOutputDirector};
pub use event::{CompileStage, ExecutionPhase, LogLevel, OutputEvent, OutputMetadata};
pub use factory::OutputDirectorFactory;
pub use file::FileOutputDirector;
pub use filter::{ComponentFilter, LevelFilter, OutputFilter, PatternFilter, PhaseFilter};
pub use formatter::{ColoredFormatter, JsonFormatter, OutputFormatter, PlainFormatter};
pub use router::{BroadcastRouter, OutputRouter, RuleBasedRouter};
pub use session::OutputSession;
pub use shared::SharedOutputDirector;
pub use sink::{BufferSink, FileSink, OutputSink, TerminalSink};
pub use source::OutputSource;
pub use stream::{OutputStream, OutputStreamType};

/// Prelude for common imports
pub mod prelude {
    pub use crate::{
        OutputEvent, OutputSource, OutputStream, OutputStreamType,
        OutputSession, OutputFilter, OutputRouter, OutputSink,
        ComponentFilter, PhaseFilter, LevelFilter,
        TerminalSink, FileSink, BufferSink,
        BroadcastRouter, RuleBasedRouter,
        PlainFormatter, ColoredFormatter, JsonFormatter,
    };
}
