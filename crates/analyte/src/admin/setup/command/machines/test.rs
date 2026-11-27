//! Runs the listen-only connection test and offers to save the observed peer.
use super::super::{Failure, Installation, Outcome, actor, ask::Ask, problem, say};
use super::{
    args::{self, Choice},
    questions::fill,
    save, summary,
};
use crate::admin::setup::{
    plan::{self, Plan},
    wizard::{Draft, listen, verdict},
};
use crate::admin::view::{Tone, format};
use analyte_i18n::{message as tr_args, text as tr};
use analyte_model::Identifier;
use std::time::Duration;

/// How often the test's state is looked at while it runs.
const TICK: Duration = Duration::from_millis(250);

pub(super) async fn test(
    installation: &Installation,
    given: Option<Identifier>,
    id: &str,
    choice: Choice,
    seconds: u64,
    show_bytes: bool,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = installation.setup.clone();
    let mut draft = args::draft(&setup, id, choice)?;
    fill(&setup, id, &mut draft, ask)?;
    let plan = plan::plan(&setup, id, &draft)?;
    let stack = plan.validated.effective().stack();
    let family = verdict::Family::of(stack);
    let (label, target) = plan::listen_target(&setup, &plan)?;
    let route = verdict::route_of(&setup, &target);
    let limits = listen::Limits {
        duration: Duration::from_secs(seconds).min(setup.seams.limits.duration),
        bytes: setup.seams.limits.bytes,
    };
    if show_bytes {
        problem(tr(
            "setup-warning-the-bytes-received-are-printed-at-the-end",
        ))?;
    }
    say(&tr_args(
        "setup-testing-for-up-to-s-nothing-is-sent-and",
        [
            ("id", (id).to_string().into()),
            ("model", (plan.model).to_string().into()),
            ("label", (label).to_string().into()),
            ("as-secs", (limits.duration.as_secs()).to_string().into()),
        ],
    ))?;
    let (session, bytes) = if show_bytes {
        let (session, bytes) = listen::Session::start_showing(label, target, limits);
        (session, Some(bytes))
    } else {
        (listen::Session::start(label, target, limits), None)
    };
    let mut told = (false, false);
    let view = loop {
        tokio::select! {
            () = tokio::time::sleep(TICK) => {}
            _ = tokio::signal::ctrl_c() => session.stop(),
        }
        let view = session.view();
        if view.connected.is_some() && !told.0 {
            told.0 = true;
            say(tr("setup-test-connected-listening"))?;
        }
        if view.closed && !told.1 {
            told.1 = true;
            say(tr("setup-the-machine-closed-the-connection"))?;
        }
        if view.end.is_some() {
            break view;
        }
    };
    let found = verdict::of_view(&view, family, &route);
    say(&format!("{}. {}", found.title, found.text))?;
    say(&verdict::received(&view, limits.bytes))?;
    say(&format!(
        "{}: {}",
        verdict::framing_characters(),
        verdict::framing(&view.framing)
    ))?;
    if let Some(bytes) = bytes
        && let Ok(bytes) = bytes.await
    {
        say(tr("setup-bytes-received-may-contain-patient-data"))?;
        say(&shown(&bytes))?;
    }
    drop(session);
    let outcome = match (&view.end, found.tone) {
        (Some(listen::End::Failed(_)), _) => Outcome::Refused,
        (_, Tone::Good) => Outcome::Done,
        (_, Tone::Attention | Tone::Problem | Tone::Neutral) => Outcome::NotSeen,
    };
    match view.peer {
        Some(peer) => offer_peer(installation, given, id, draft, &plan, peer, ask).await,
        None => Ok(outcome),
    }
    .map(|saved| {
        if saved == Outcome::Done {
            outcome
        } else {
            saved
        }
    })
}

/// After a test of a machine that connects to Analyte, names the device that
/// connected and, at a terminal, offers to accept only that device from now
/// on, through the Save step's code (ADR 0026); a script is told the command.
async fn offer_peer(
    installation: &Installation,
    given: Option<Identifier>,
    id: &str,
    mut draft: Draft,
    plan: &Plan,
    peer: std::net::IpAddr,
    ask: Ask,
) -> Result<Outcome, Failure> {
    say(&tr_args(
        "setup-the-device-that-connected",
        [("peer", (peer).to_string().into())],
    ))?;
    if plan.peers.as_ref().is_some_and(|peers| peers.allows(peer)) {
        say(tr("setup-it-is-one-of-the-devices-this-machine-accepts"))?;
        return Ok(Outcome::Done);
    }
    let command = tr_args(
        "setup-analyte-machines-set-allowed-peer",
        [
            ("id", (id).to_string().into()),
            ("peer", (peer).to_string().into()),
        ],
    );
    if ask == Ask::Never {
        say(&tr_args(
            "setup-check-that-this-is-the-analyzer-s-own-address",
            [("command", (command).to_string().into())],
        ))?;
        return Ok(Outcome::Done);
    }
    say(tr("setup-check-that-this-is-the-analyzer-s-own-address-2"))?;
    if !ask.confirm(&tr_args(
        "setup-only-accept-this-device-from-now-on",
        [("peer", (peer).to_string().into())],
    ))? {
        say(&tr_args(
            "setup-nothing-was-changed-to-do-it-later",
            [("command", (command).to_string().into())],
        ))?;
        return Ok(Outcome::Done);
    }
    draft.allowed_peers = Some(peer.to_string());
    let actor = actor(given, ask)?;
    let plan = plan::checked(&installation.setup, id, &draft)?;
    summary(id, &plan)?;
    save(installation, &actor, id, draft).await
}

/// The received bytes as text: printable ASCII as it is, every other byte by
/// name in angle brackets, a new line after each CR or LF.
fn shown(bytes: &[u8]) -> String {
    let mut text = String::new();
    for &byte in bytes {
        match byte {
            b' '..=b'~' => text.push(char::from(byte)),
            0x00..=0x1f | 0x7f => {
                // Already in angle brackets, such as <STX>.
                text.push_str(&format::control_names(&[byte]));
                if matches!(byte, b'\r' | b'\n') {
                    text.push('\n');
                }
            }
            other => text.push_str(&format!("<{other:02X}>")),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::shown;

    // Owner request 2026-11-21 (command line): --show-bytes prints control
    // bytes by name and starts a new line after each segment end.
    #[test]
    fn shown_bytes_name_control_characters() {
        assert_eq!(shown(b"\x021H|\\^&\r\x03"), "<STX>1H|\\^&<CR>\n<ETX>");
        assert_eq!(shown(&[0xC3, b'a']), "<C3>a");
    }
}
