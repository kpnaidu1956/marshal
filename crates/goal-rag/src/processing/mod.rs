//! Background processing with job queue and progress tracking

mod file_tier;
mod job_queue;
mod worker;

pub use file_tier::{
    FileCharacteristics, FileTier, ParserStrategy, PdfAnalysis,
};
pub use job_queue::{
    FileData, FileError, FileProcessingStatus, FileProgressRecord, Job, JobQueue, JobProgress,
    JobStatus, ParserAttemptRecord, ProcessingOptions, ProcessingStage, QueueStats,
    // GCS-based job types (new architecture)
    GcsFileRef, GcsProcessingOptions, GcsJob, ProcessingJob,
};
pub use worker::ProcessingWorker;
