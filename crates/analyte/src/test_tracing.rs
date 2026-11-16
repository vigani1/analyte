//! One process-wide subscriber keeps log-capture callsites interested across test threads.
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct Writer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut output = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("test tracing buffer poisoned"))?;
        assert!(
            output.len().saturating_add(bytes.len()) <= 8 * 1024 * 1024,
            "test tracing capture exceeded its bounded capacity"
        );
        output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Initializes the shared subscriber before any thread-local output capture.
/// The mutex protects only the bounded capture bytes, never an asynchronous operation.
pub(crate) fn init() -> Arc<Mutex<Vec<u8>>> {
    static OUTPUT: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    OUTPUT
        .get_or_init(|| {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let output = bytes.clone();
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .without_time()
                .with_ansi(false)
                .with_writer(move || Writer(output.clone()))
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .expect("the shared test tracing fixture owns the process-wide subscriber");
            bytes
        })
        .clone()
}
