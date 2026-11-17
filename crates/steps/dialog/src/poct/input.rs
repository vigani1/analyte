//! Frames journaled POCT input and retains every rejected or unfinished root.
use super::{Action, Dialog, Error, Event};
use std::time::Duration;
impl Dialog {
    /// Drains unfinished evidence when the host cancels or closes the connection.
    pub fn take_input(&mut self) -> Vec<u8> {
        self.input.take_pending()
    }
    pub(super) fn input_byte(&mut self, now: Duration, byte: u8) -> Result<Vec<Action>, Error> {
        let wire = match self.input.push_byte(byte) {
            Ok(Some(wire)) => wire,
            Ok(None) => {
                if byte != b'>' {
                    return Ok(Vec::new());
                }
                let mut actions = self.step(now, Event::Tick)?;
                if actions
                    .iter()
                    .any(|action| matches!(action, Action::Close { .. }))
                {
                    actions.insert(0, Action::ParseDeadline(self.take_input()));
                }
                return Ok(actions);
            }
            Err(analyte_framing::poct::Error::Limit) => {
                return Ok(vec![Action::ParseLimit(self.take_input())]);
            }
            Err(
                analyte_framing::poct::Error::Incomplete
                | analyte_framing::poct::Error::Malformed(_)
                | analyte_framing::poct::Error::Depth
                | analyte_framing::poct::Error::Unbalanced,
            ) => self.take_input(),
        };
        let mut actions = self.step(now, Event::Tick)?;
        if actions
            .iter()
            .any(|action| matches!(action, Action::Close { .. }))
        {
            actions.insert(0, Action::ParseDeadline(wire));
        } else {
            actions.push(Action::Parse(wire));
        }
        Ok(actions)
    }
}
