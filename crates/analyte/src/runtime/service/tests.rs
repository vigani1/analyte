//! Tests service supervision and explicit store ownership.

use super::*;

#[tokio::test]
async fn rule7_service_close_waits_for_store_destruction() -> Result<(), Error> {
    struct Space {
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl FreeSpace for Space {
        fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }
    impl Drop for Space {
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
        analyte_store::Limits {
            reserve_bytes: 1,
            database_bytes: 100_000_000,
            record_bytes: 1_048_576,
            page_rows: 100,
            busy_timeout: Duration::from_millis(100),
        },
        Box::new(Space {
            entered: Some(entered),
            release: released,
        }),
        1,
        deadline(),
    )
    .await?;
    let close = close_store(Arc::new(store));
    tokio::pin!(close);
    tokio::select! {
        result = &mut close => panic!("close returned before store destruction: {result:?}"),
        result = observed => result?,
    }
    use std::future::Future as _;
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(close.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    release.send(())?;
    close.await?;
    Ok(())
}

#[tokio::test]
async fn plan11_supervision_preserves_stop_failure_and_refuses_unexpected_admin_exit() {
    let mut alerts = JoinSet::new();
    let mut admin = JoinSet::new();
    let mut deliveries = JoinSet::new();
    let mut machines = JoinSet::new();
    let result = supervise(
        async { Err(crate::invalid("synthetic stop signal failure")) },
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
