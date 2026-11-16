//! Tests rejection of malformed inputs without losing evidence.
use super::*;

fn segment(name: &str, count: usize, values: &[(usize, &str)]) -> Result<String, &'static str> {
    let mut fields = vec![""; count];
    for (field, value) in values {
        let slot = field
            .checked_sub(1)
            .and_then(|index| fields.get_mut(index))
            .ok_or("invalid synthetic field position")?;
        *slot = value;
    }
    Ok(format!("{name}|{}\r", fields.join("|")))
}

mod scenarios_1;

mod scenarios_2;
