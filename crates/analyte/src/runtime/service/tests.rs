//! Tests service supervision and explicit store ownership.

use super::*;

// Safety rule 7 (ADR 0003)
#[tokio::test]
async fn close_waits_for_store_destruction() -> Result<(), crate::Error> {
    struct Observer {
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl Drop for Observer {
        fn drop(&mut self) {
            let _ = self.entered.take().expect("one destruction").send(());
            self.release
                .recv_timeout(Duration::from_secs(30))
                .expect("test releases store");
        }
    }
    let temp = tempfile::tempdir()?;
    let (entered, observed) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let store = Handle::start(
        temp.path().join("close.db"),
        {
            let mut limits = analyte_fixtures::store_limits();
            limits.reserve_bytes = 1;
            limits.record_bytes = 1_048_576;
            limits.busy_timeout = Duration::from_millis(100);
            limits
        },
        Box::new(analyte_fixtures::ObservedSpace {
            space: analyte_fixtures::UnlimitedSpace,
            observer: Observer {
                entered: Some(entered),
                release: released,
            },
        }),
        1,
        crate::test_startup_deadline()?,
    )
    .await?;
    let close = close_store(Arc::new(store));
    tokio::pin!(close);
    tokio::select! {
        result = &mut close => panic!("close returned before store destruction: {result:?}"),
        result = observed => result?,
    }
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(close.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    release.send(())?;
    close.await?;
    Ok(())
}

// Plan §11
#[tokio::test]
async fn supervision_keeps_stop_failure_and_refuses_admin_exit() {
    let mut alerts = JoinSet::new();
    let mut admin = JoinSet::new();
    let mut deliveries = JoinSet::new();
    let mut machines = JoinSet::new();
    let result = supervise(
        async {
            Err(Error::Io(std::io::Error::other(
                "synthetic stop signal failure",
            )))
        },
        &mut alerts,
        &mut admin,
        &mut deliveries,
        &mut machines,
    )
    .await;
    assert_eq!(
        result.expect_err("signal failure").to_string(),
        "synthetic stop signal failure"
    );
    admin.spawn(async { Ok(()) });
    let result = supervise(
        std::future::pending(),
        &mut alerts,
        &mut admin,
        &mut deliveries,
        &mut machines,
    )
    .await;
    assert_eq!(
        result.expect_err("admin exit must be visible").to_string(),
        "admin owner stopped unexpectedly"
    );
    assert!(admin.is_empty());
    supervise(
        async { Ok(()) },
        &mut alerts,
        &mut admin,
        &mut deliveries,
        &mut machines,
    )
    .await
    .expect("normal stop succeeds without owners");
}
