//! Dedicated serial owner with explicit join, including on cancellation.
use super::{Error, Handle, Limits, Machine, invalid};
use analyte_transport::{Cancellation, serial::Settings};
use std::{
    thread,
    time::{Duration, Instant},
};
use tokio::sync::watch;

struct Owner {
    cancellation: Cancellation,
    thread: Option<thread::JoinHandle<Result<(), analyte_transport::Error>>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Owner {
    async fn close(mut self) -> Result<(), Error> {
        self.cancellation.cancel();
        let deadline = crate::deadline();
        while self.thread.as_ref().is_some_and(|t| !t.is_finished()) {
            if Instant::now() >= deadline {
                return Err(invalid("serial owner shutdown timed out"));
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| invalid("serial owner panicked"))??;
        }
        Ok(())
    }
}

pub(super) async fn run(
    machine: &Machine,
    store: &Handle,
    limits: Limits,
    stop: watch::Receiver<bool>,
    mut closing: super::Closing,
) -> Result<(), Error> {
    let serial = machine
        .snapshot
        .effective()
        .machine()
        .transport
        .serial
        .as_ref()
        .ok_or_else(|| invalid("serial settings absent"))?;
    let parity = match serial.parity {
        analyte_config::Parity::None => serialport::Parity::None,
        analyte_config::Parity::Odd => serialport::Parity::Odd,
        analyte_config::Parity::Even => serialport::Parity::Even,
    };
    let flow = match serial.flow_control {
        analyte_config::FlowControl::None => serialport::FlowControl::None,
        analyte_config::FlowControl::Hardware => serialport::FlowControl::Hardware,
        analyte_config::FlowControl::Software => serialport::FlowControl::Software,
    };
    let settings = Settings::new(
        machine.snapshot.effective().site().endpoint.clone(),
        serial.baud,
        serial
            .data_bits
            .try_into()
            .map_err(|()| invalid("invalid serial data bits"))?,
        parity,
        serial
            .stop_bits
            .try_into()
            .map_err(|()| invalid("invalid serial stop bits"))?,
        flow,
        Duration::from_millis(100),
    )?;
    let (mut conn, side) = analyte_transport::connection(limits);
    let cancellation = conn.cancellation();
    let retry = super::retry()?;
    let thread = thread::Builder::new()
        .name("analyte-serial".into())
        .spawn(move || analyte_transport::serial::run_system(&settings, side, retry))?;
    let owner = Owner {
        cancellation,
        thread: Some(thread),
    };
    let result = loop {
        let result = super::session(machine, store, &mut conn, stop.clone()).await;
        if result.is_err() || *stop.borrow() {
            break result;
        }
    };
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    drop(conn);
    owner.close().await?;
    result
}
