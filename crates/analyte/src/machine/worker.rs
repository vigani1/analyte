//! Reports dropped machine workers without logging errors or panic payloads.
pub(super) fn report_join<E>(worker: &'static str, result: &std::thread::Result<Result<(), E>>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => tracing::error!(
            worker,
            failure = "worker_error",
            "machine worker failed during shutdown"
        ),
        Err(_) => tracing::error!(
            worker,
            failure = "worker_panic",
            "machine worker panicked during shutdown"
        ),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("capture lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    pub(in crate::machine) fn capture(action: impl FnOnce()) -> String {
        crate::runtime::test_tracing::init();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let writer = Capture(bytes.clone());
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, action);
        String::from_utf8(bytes.lock().expect("capture lock").clone()).expect("UTF-8 logs")
    }
}
