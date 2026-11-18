//! Runs the Windows service control adapter.
use crate::runtime::Error as Failure;
use crate::runtime::Error;
use crate::{installation::config::Program, runtime::service};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use windows_services::{Command, Service, State};

enum Event {
    Finished,
    Deadline,
}

/// Runs only under the SCM; stop and OS shutdown share the graceful service path.
/// The SCM exposes a normal stop on deadline; the process exit code is 3.
/// # Errors
/// Reports dispatcher, startup, worker and graceful shutdown errors.
pub fn run(config: PathBuf, bound: Duration) -> Result<(), Error> {
    if !(Duration::from_secs(30)..=Duration::from_secs(300)).contains(&bound) {
        return Err(Failure::InvalidShutdownBound);
    }
    let (start, started) = mpsc::sync_channel(1);
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let (events, received) = mpsc::sync_channel(2);
    let received = Mutex::new(received);
    let (reported, report) = mpsc::sync_channel(1);
    let report = Mutex::new(report);
    let requested = Arc::new(AtomicBool::new(false));
    let stopping = requested.clone();
    let deadline_events = events.clone();
    let notice = Arc::new(move || {
        if stopping.load(Ordering::Acquire) {
            let _ = deadline_events.send(Event::Deadline);
            // The SCM callback only sets status and acknowledges; never waits on an owner.
            if let Ok(report) = report.lock() {
                let _ = report.recv_timeout(Duration::from_millis(250));
            }
        }
    });
    let watchdog = Arc::new(crate::runtime::shutdown::Watchdog::new(
        bound,
        Some(notice),
    )?);
    std::thread::scope(|scope| {
        let runtime_watchdog = watchdog.clone();
        let stopping = requested.clone();
        let worker = scope.spawn(move || -> Result<(), Error> {
            if started.recv().is_err() {
                return Ok(());
            }
            let startup = runtime_watchdog.component("startup".into());
            let result = run_host(&config, bound, stopped, runtime_watchdog, startup);
            if !stopping.load(Ordering::Acquire) {
                if let Err(error) = &result {
                    tracing::error!(%error, "service failed before an SCM stop request");
                } else {
                    tracing::error!("service stopped unexpectedly before an SCM stop request");
                }
                std::process::exit(1);
            }
            if events.send(Event::Finished).is_err() {
                // The dispatcher can close its receiver before the runtime owner exits.
                tracing::debug!("SCM completion receiver closed after dispatcher stopped");
            }
            result
        });
        let mut start = Some(start);
        let mut stop = Some(stop);
        let dispatch = Service::new()
            .can_stop()
            .run(|service, command| match command {
                Command::Start => {
                    if let Some(start) = start.take() {
                        let _ = start.send(());
                    }
                }
                Command::Stop => {
                    requested.store(true, Ordering::Release);
                    watchdog.arm();
                    if let Some(stop) = stop.take() {
                        let _ = stop.send(());
                    }
                    if let Ok(received) = received.lock() {
                        report_stop(&received, &reported, || service.set_state(State::Stopped));
                    }
                }
                Command::Pause | Command::Resume | Command::Extended(_) => {}
            });
        // Wake the owned worker when dispatch failed outside the SCM.
        drop(start);
        let result = worker
            .join()
            .map_err(|_| Failure::ScmRuntimeOwnerPanicked)?;
        dispatch.map_err(|source| {
            Failure::ScmDispatcherUnavailable(crate::runtime::error::DispatchCause(source))
        })?;
        result
    })
}

fn report_stop(
    events: &mpsc::Receiver<Event>,
    reported: &mpsc::SyncSender<()>,
    stopped: impl FnOnce(),
) {
    let event = events.recv();
    stopped();
    if matches!(event, Ok(Event::Deadline)) {
        let _ = reported.send(());
    }
}

fn run_host(
    config: &std::path::Path,
    bound: Duration,
    stopped: tokio::sync::oneshot::Receiver<()>,
    runtime_watchdog: Arc<crate::runtime::shutdown::Watchdog>,
    startup: crate::runtime::shutdown::Component,
) -> Result<(), Error> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let program = Program::load(config)?;
        // Install scripts restrict this directory before starting the service.
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(program.data_directory.join("analyte.log"))?;
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(log)
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init()
            .map_err(Error::Tracing)?;
        let secrets = crate::installation::secrets::Environment::load(&program)?;
        drop(startup);
        service::serve_bounded(
            program,
            async { stopped.await.map_err(Failure::ScmStopSenderClosed) },
            secrets,
            bound,
            Some(runtime_watchdog),
        )
        .await
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    // Plan §11
    #[test]
    fn scm_reports_stopped_before_forced_exit_ack() -> Result<(), Error> {
        let (events, received) = mpsc::sync_channel(1);
        let (reported, report) = mpsc::sync_channel(1);
        events.send(Event::Deadline)?;
        let mut stopped = false;
        report_stop(&received, &reported, || {
            assert!(report.try_recv().is_err());
            stopped = true;
        });
        assert!(stopped);
        report.recv_timeout(Duration::from_secs(1))?;
        Ok(())
    }
    // Plan §11
    #[test]
    fn scm_graceful_completion_reports_stopped() -> Result<(), Error> {
        let (events, received) = mpsc::sync_channel(1);
        let (reported, report) = mpsc::sync_channel(1);
        events.send(Event::Finished)?;
        let mut stopped = false;
        report_stop(&received, &reported, || stopped = true);
        assert!(stopped);
        assert!(report.try_recv().is_err());
        Ok(())
    }
    // Plan §11
    #[test]
    fn scm_closed_channel_reports_stopped_without_ack() {
        let (events, received) = mpsc::sync_channel(1);
        let (reported, report) = mpsc::sync_channel(1);
        drop(events);
        let mut stopped = false;
        report_stop(&received, &reported, || stopped = true);
        assert!(stopped);
        assert!(report.try_recv().is_err());
    }

    // Plan §11
    #[test]
    fn scm_invalid_bounds_refused_before_dispatch() {
        for seconds in [0, 29, 301] {
            let error = run(PathBuf::from("unused-config"), Duration::from_secs(seconds))
                .expect_err("invalid bound must not reach SCM");
            assert!(error.to_string().contains("shutdown bound must be"));
        }
    }
}
