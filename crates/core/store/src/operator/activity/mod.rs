//! Reads the Activity feed, newest first, from the tables that already record what Analyte did: receipts, parked items, delivery attempts, machine order history and the audit trail.
//!
//! Nothing here is a new record of events: each line is a row that was stored
//! anyway, with the time stored beside it. A row without a stored time (a
//! parked item from before schema 16) sorts after every dated line and keeps
//! `None`, never a guessed time. No bytes, values, test codes or typed
//! reasons leave the store; sample IDs do, as stored.
use crate::{OperatorError as Error, Parked, RecentOutcome, Store};
use analyte_model::OrderState;
use rusqlite::{OptionalExtension as _, params};
use serde::Deserialize;
#[cfg(test)]
mod tests;

/// Which kind of line a filter keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityGroup {
    /// Messages stored and results delivered.
    Results,
    /// Orders sent to machines, and how each machine took them.
    Orders,
    /// Items parked for review and deliveries that did not go through.
    Problems,
    /// What people or Analyte changed: settings, holds, resolved items.
    Changes,
}

impl ActivityGroup {
    /// Every group, in the order a page lists its filters.
    pub const ALL: [Self; 4] = [Self::Results, Self::Orders, Self::Problems, Self::Changes];
}

/// Where a page of the feed starts: the position of the last line shown.
/// Lines sort by time, then by where they are stored, then by their row, so
/// two lines at the same second keep one order on every page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityCursor {
    /// The line's time in Unix seconds; [`i64::MIN`] for a line with none.
    pub at: i64,
    /// Which table the line comes from.
    pub source: i64,
    /// Its row in that table.
    pub key: i64,
}

impl ActivityCursor {
    /// Before every line: the first page.
    pub const NEWEST: Self = Self {
        at: i64::MAX,
        source: i64::MAX,
        key: i64::MAX,
    };
}

/// One line of the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityLine {
    /// Its position, for the next page.
    pub cursor: ActivityCursor,
    /// When it happened, in Unix seconds; none when no time was stored.
    pub at: Option<i64>,
    /// The machine it is about, as stored; none when it names no single machine.
    pub machine: Option<String>,
    /// What happened.
    pub what: Activity,
}

/// What a line says happened, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// A message from a machine was stored, with what its latest reading
    /// produced; none while it was never read.
    Stored(Option<RecentOutcome>),
    /// An item was parked for review.
    Parked {
        /// Its Needs review number.
        item: i64,
        /// Its kind, reason and links, as stored.
        details: Parked,
    },
    /// An item was resolved, by the audited action stored with it.
    Resolved {
        /// Its Needs review number.
        item: i64,
        /// The action, such as `parked-resolve`.
        action: String,
    },
    /// Analyte tried to deliver to a lab app.
    Attempt {
        /// The lab app, as stored.
        lab_app: String,
        /// How the try ended.
        outcome: AttemptOutcome,
        /// What the delivery carries.
        carries: Carries,
    },
    /// A machine order reached a new state.
    Order {
        /// The lab app's order number.
        order: String,
        /// The sample of the lab app's copy, when one is kept.
        sample: Option<String>,
        /// How many tests it holds; none when an older version did not record them.
        tests: Option<usize>,
        /// Its new state.
        state: OrderState,
    },
    /// An audited change, such as a machine's settings saved.
    Changed {
        /// Who, as recorded.
        actor: String,
        /// The action, such as `setup-save`.
        action: String,
        /// The first thing it names, such as a machine or lab app; none when none.
        target: Option<String>,
    },
}

/// How one delivery try ended, as its attempt row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// The lab app accepted it.
    Delivered,
    /// It did not go through; Analyte tries again after this Unix second.
    RetryAt(i64),
    /// The try was cut off; Analyte tries again with the same bytes.
    Interrupted,
    /// The lab app rejected it, so it waits for review.
    Rejected,
    /// The lab app's settings need repair, so sending stopped.
    Paused,
    /// The lab app holds different content under the same delivery ID.
    Conflict,
}

/// What a delivery carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carries {
    /// Results, with what their reading produced; none when it is no longer kept.
    Results(Option<RecentOutcome>),
    /// A message about an order.
    OrderMessage {
        /// The lab app's order number.
        order: String,
    },
}

/// How far each table the live pages watch has grown: its newest row (or,
/// for open review items, their count). Two equal marks mean nothing those
/// pages show was stored in between. Each value is one index lookup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActivityMarks {
    /// Messages stored.
    pub receipts: i64,
    /// Readings of messages, first or again.
    pub readings: i64,
    /// Items parked for review.
    pub parked: i64,
    /// Items waiting for review now; it falls when one is resolved.
    pub open: i64,
    /// Deliveries queued.
    pub deliveries: i64,
    /// Delivery tries.
    pub attempts: i64,
    /// Machine order snapshots.
    pub orders: i64,
    /// Audited changes.
    pub audit: i64,
}

/// The machine a parked item `x` names: through its first target, when it has
/// one, that is a stored message, a delivery of one, a machine order or a
/// transfer, else through the poll intent it belongs to.
macro_rules! parked_machine {
    () => {
        concat!(
            "coalesce(CASE WHEN json_type(x.evidence,'$.targets[0]') IS NOT NULL THEN coalesce(",
            "(SELECT json_extract(r.metadata,'$.machine') FROM receipts r WHERE r.id=json_quote(json_extract(x.evidence,'$.targets[0]'))),",
            "(SELECT json_extract(r.metadata,'$.machine') FROM outbox o JOIN receipts r ON r.id=o.receipt WHERE o.id=json_quote(json_extract(x.evidence,'$.targets[0]'))),",
            "(SELECT m.machine FROM machine_orders m WHERE m.id=CAST(json_extract(x.evidence,'$.targets[0]') AS INTEGER) AND CAST(m.id AS TEXT)=json_extract(x.evidence,'$.targets[0]')),",
            "(SELECT json_extract(f.evidence,'$.machine') FROM frame_provenance f WHERE f.transfer=json_extract(x.evidence,'$.targets[0]'))",
            ") END,",
            "(SELECT i.machine FROM poll_intents i WHERE i.parked=x.id))"
        )
    };
}

/// The machine a delivery attempt `x` is about: its message's, or the one
/// machine its order was sent to.
macro_rules! attempt_machine {
    () => {
        concat!(
            "(SELECT coalesce(",
            "(SELECT json_extract(r.metadata,'$.machine') FROM receipts r WHERE r.id=o.receipt),",
            "(SELECT CASE WHEN count(DISTINCT m.machine)=1 THEN min(m.machine) END FROM machine_orders m WHERE m.order_id=o.order_id AND m.destination=o.destination)",
            ") FROM outbox o WHERE o.id=x.delivery)"
        )
    };
}

/// Where a machine's attempts `x` are found through its own rows: its
/// messages' deliveries, and the order messages of orders sent to it. Every
/// attempt naming the machine is in one of them.
const ATTEMPTS_BY_MACHINE: &[&str] = &[
    "receipts r CROSS JOIN outbox o CROSS JOIN attempts x WHERE json_extract(r.metadata,'$.machine')=?1 AND o.receipt=r.id AND x.delivery=o.id",
    "machine_orders m CROSS JOIN outbox o CROSS JOIN attempts x WHERE m.machine=?1 AND o.destination=m.destination AND o.kind='order_event' AND o.ordering_key=m.order_id AND x.delivery=o.id",
];

/// The state of the order snapshot stored before `x` for the same order.
macro_rules! previous_state {
    () => {
        "(SELECT json_extract(p.evidence,'$.state') FROM machine_order_history p WHERE p.machine_order=x.machine_order AND p.sequence<x.sequence ORDER BY p.sequence DESC LIMIT 1)"
    };
}

/// One table the feed reads lines from, as SQL over the alias `x`. `at` is
/// the line's stored time, NULL when none was stored; schema 17 indexes it
/// (after the machine too, where the machine is a stored value), so a source
/// gives its newest lines before a cursor without reading the rest.
struct Source {
    /// Which table: fixes the order of lines stored in the same second.
    number: i64,
    /// The filter its lines belong to.
    group: ActivityGroup,
    from: &'static str,
    at: &'static str,
    /// The row, unique within `number`.
    key: &'static str,
    machine: &'static str,
    /// What every line of it meets besides its time and machine.
    keep: &'static str,
    /// Whether `at` can be NULL.
    undated: bool,
    /// Where one machine's rows are found without reading the others, when
    /// its machine is not indexed: each a FROM and WHERE ending in a condition.
    by_machine: &'static [&'static str],
}

/// Every source of lines. Read-only audit entries (opening original bytes,
/// listing from the command line) are not changes, and an audit entry stored
/// as an item's resolution is already its Resolved line. A repeated order
/// snapshot in the same state is not a line. Attempts are read as two
/// sources with one number, accepted and not, so each filter reads one.
const SOURCES: [Source; 7] = [
    Source {
        number: 0,
        group: ActivityGroup::Results,
        from: "receipts x",
        at: "unixepoch(json_extract(x.metadata,'$.received_at'))",
        key: "x.rowid",
        machine: "json_extract(x.metadata,'$.machine')",
        keep: "1",
        undated: true,
        by_machine: &[],
    },
    Source {
        number: 1,
        group: ActivityGroup::Problems,
        from: "parked x",
        at: "x.parked_at",
        key: "x.id",
        machine: parked_machine!(),
        keep: "1",
        undated: true,
        by_machine: &[],
    },
    Source {
        number: 2,
        group: ActivityGroup::Changes,
        from: "parked x",
        at: "CASE WHEN json_valid(x.resolution) THEN json_extract(x.resolution,'$.time') END",
        key: "x.id",
        machine: parked_machine!(),
        keep: "x.resolution IS NOT NULL",
        undated: true,
        by_machine: &[],
    },
    Source {
        number: 3,
        group: ActivityGroup::Results,
        from: "attempts x",
        at: "x.time",
        key: "x.sequence",
        machine: attempt_machine!(),
        keep: "(x.outcome='\"Done\"')=1 AND EXISTS(SELECT 1 FROM outbox o WHERE o.id=x.delivery)",
        undated: false,
        by_machine: ATTEMPTS_BY_MACHINE,
    },
    Source {
        number: 3,
        group: ActivityGroup::Problems,
        from: "attempts x",
        at: "x.time",
        key: "x.sequence",
        machine: attempt_machine!(),
        keep: "(x.outcome='\"Done\"')=0 AND EXISTS(SELECT 1 FROM outbox o WHERE o.id=x.delivery)",
        undated: false,
        by_machine: ATTEMPTS_BY_MACHINE,
    },
    Source {
        number: 4,
        group: ActivityGroup::Orders,
        from: "machine_order_history x",
        at: "json_extract(x.evidence,'$.updated_at')",
        key: "x.sequence",
        machine: "json_extract(x.evidence,'$.key.machine')",
        keep: concat!(
            "(",
            previous_state!(),
            " IS NULL OR ",
            previous_state!(),
            "!=json_extract(x.evidence,'$.state'))"
        ),
        undated: true,
        by_machine: &[],
    },
    Source {
        number: 5,
        group: ActivityGroup::Changes,
        from: "audit x",
        at: "json_extract(x.evidence,'$.time')",
        key: "x.sequence",
        machine: "CASE WHEN json_extract(x.evidence,'$.action') LIKE 'setup-%' THEN json_extract(x.evidence,'$.targets[0]') END",
        keep: concat!(
            "json_extract(x.evidence,'$.action') NOT IN ('admin-raw-view','deliveries-list','orders-list') ",
            "AND NOT EXISTS(SELECT 1 FROM parked p WHERE p.resolution IS NOT NULL ",
            "AND (CASE WHEN json_valid(p.resolution) THEN json_extract(p.resolution,'$.time') END) IS json_extract(x.evidence,'$.time') ",
            "AND p.resolution=x.evidence)"
        ),
        undated: true,
        by_machine: &[],
    },
];

/// One line's position, time and machine as a source reads them:
/// `(sort, source, key, at, machine)`, where `sort` is `at` or [`i64::MIN`].
type Row = (i64, i64, i64, Option<i64>, Option<String>);

/// Binds `values` to `statement` in order, as many as it numbers: every
/// statement here numbers `?1` machine, `?2` time, `?3` row and `?4` limit
/// (or `?3` end of day), and uses those it needs.
fn bind(
    statement: &mut rusqlite::Statement<'_>,
    values: &[&dyn rusqlite::ToSql],
) -> Result<(), Error> {
    for index in 1..=statement.parameter_count() {
        let value = values
            .get(index.wrapping_sub(1))
            .ok_or(Error::InvalidOperatorInput)?;
        statement.raw_bind_parameter(index, value)?;
    }
    Ok(())
}

/// Newest first, as the feed sorts.
fn newest_first(a: &Row, b: &Row) -> std::cmp::Ordering {
    (b.0, b.1, b.2).cmp(&(a.0, a.1, a.2))
}

impl Source {
    /// The machine condition, bound to `?1`, when a machine is asked for.
    fn machine_filter(&self, machine: Option<&str>) -> String {
        machine.map_or_else(String::new, |_| format!(" AND {}=?1", self.machine))
    }

    /// One line as its statement reads it: time, row, machine.
    fn row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
        let at: Option<i64> = row.get(0)?;
        Ok((
            at.unwrap_or(i64::MIN),
            self.number,
            row.get(1)?,
            at,
            row.get(2)?,
        ))
    }

    /// Every line `sql` reads with `values`.
    fn read(
        &self,
        db: &rusqlite::Connection,
        sql: &str,
        values: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<Row>, Error> {
        let mut statement = db.prepare(sql)?;
        bind(&mut statement, values)?;
        let mut rows = statement.raw_query();
        let mut lines = Vec::new();
        while let Some(row) = rows.next()? {
            lines.push(self.row(row)?);
        }
        Ok(lines)
    }

    /// Its newest lines before `before`, newest first, at most `limit`.
    fn lines(
        &self,
        db: &rusqlite::Connection,
        machine: Option<&str>,
        before: ActivityCursor,
        limit: i64,
    ) -> Result<Vec<Row>, Error> {
        let Self {
            from,
            at,
            key,
            machine: named,
            keep,
            ..
        } = self;
        let filter = self.machine_filter(machine);
        let wanted = usize::try_from(limit).map_err(|_| Error::InvalidOperatorInput)?;
        // The rows of this source still after the cursor in the cursor's own
        // second: all of them, those before its row, or none.
        let same_second = match self.number.cmp(&before.source) {
            std::cmp::Ordering::Less => Some(i64::MAX),
            std::cmp::Ordering::Equal => before.key.checked_sub(1),
            std::cmp::Ordering::Greater => None,
        };
        let mut rows = Vec::new();
        if before.at != i64::MIN {
            let dated = format!("{at}<=?2 AND ({at}<?2 OR {key}<=?3) AND {keep}");
            let values: [&dyn rusqlite::ToSql; 4] = [&machine, &before.at, &same_second, &limit];
            rows = match machine {
                Some(machine) if !self.by_machine.is_empty() => {
                    self.raced(db, &dated, &values, machine, wanted)?
                }
                _ => self.read(
                    db,
                    &format!(
                        "SELECT {at},{key},{named} FROM {from} WHERE {dated}{filter} ORDER BY {at} DESC,{key} DESC LIMIT ?4"
                    ),
                    &values,
                )?,
            };
        }
        // Lines with no stored time sort after every dated one.
        let undated = if before.at == i64::MIN {
            same_second
        } else {
            Some(i64::MAX)
        };
        if let Some(row) = undated.filter(|_| self.undated && rows.len() < wanted) {
            rows.extend(self.read(
                db,
                &format!(
                    "SELECT NULL,{key},{named} FROM {from} WHERE {at} IS NULL AND {key}<=?3 AND {keep}{filter} ORDER BY {key} DESC LIMIT ?4"
                ),
                &[&machine, &before.at, &row, &limit],
            )?);
        }
        Ok(rows)
    }

    /// One machine's newest `wanted` lines meeting `dated`, read two ways at
    /// once, one row of each in turn: newest first over the whole table,
    /// comparing each row's machine, and every row of the machine through
    /// `by_machine`, sorted at the end. The first way to finish gives the
    /// answer, so a busy machine costs about a page and a quiet one about its
    /// own rows, never the whole table.
    fn raced(
        &self,
        db: &rusqlite::Connection,
        dated: &str,
        values: &[&dyn rusqlite::ToSql],
        machine: &str,
        wanted: usize,
    ) -> Result<Vec<Row>, Error> {
        let Self {
            from,
            at,
            key,
            machine: named,
            by_machine,
            ..
        } = self;
        if wanted == 0 {
            return Ok(Vec::new());
        }
        let mut newest = db.prepare(&format!(
            "SELECT {at},{key},{named} FROM {from} WHERE {dated} ORDER BY {at} DESC,{key} DESC"
        ))?;
        bind(&mut newest, values)?;
        let mut own = db.prepare(
            &by_machine
                .iter()
                .map(|found| {
                    format!("SELECT {at},{key},{named} FROM {found} AND {dated} AND {named}=?1")
                })
                .collect::<Vec<_>>()
                .join(" UNION ALL "),
        )?;
        bind(&mut own, values)?;
        let (mut newest, mut own) = (newest.raw_query(), own.raw_query());
        let (mut walked, mut found) = (Vec::new(), Vec::new());
        loop {
            let Some(row) = newest.next()? else {
                return Ok(walked);
            };
            let line = self.row(row)?;
            if line.4.as_deref() == Some(machine) {
                walked.push(line);
                if walked.len() >= wanted {
                    return Ok(walked);
                }
            }
            let Some(row) = own.next()? else {
                found.sort_unstable_by(newest_first);
                found.truncate(wanted);
                return Ok(found);
            };
            found.push(self.row(row)?);
        }
    }

    /// How many of its lines have a time in `day` (Unix seconds `from..until`).
    fn count(
        &self,
        db: &rusqlite::Connection,
        machine: Option<&str>,
        day: (i64, i64),
    ) -> Result<i64, Error> {
        let Self { from, at, keep, .. } = self;
        let filter = self.machine_filter(machine);
        let mut statement = db.prepare(&format!(
            "SELECT count(*) FROM {from} WHERE {at}>=?2 AND {at}<?3 AND {keep}{filter}"
        ))?;
        bind(&mut statement, &[&machine, &day.0, &day.1])?;
        let mut rows = statement.raw_query();
        let row = rows.next()?.ok_or(Error::Corruption)?;
        Ok(row.get(0)?)
    }
}

impl Store {
    /// At most `limit` lines (and at most one store page) before `before`,
    /// newest first, for `machine` and `group` when given. Reading changes
    /// nothing.
    ///
    /// Each source gives at most `limit` of its own newest lines before the
    /// cursor, through its time index, and the page is the newest of those, so
    /// the cost follows the page, not what is kept. A machine that is not a
    /// stored value (a parked item's, an attempt's or an audit entry's) is
    /// compared line by line from the newest, until the page is full.
    /// # Errors
    /// Reports database failures, bounds or corrupt stored evidence.
    pub fn activity(
        &self,
        machine: Option<&str>,
        group: Option<ActivityGroup>,
        before: ActivityCursor,
        limit: usize,
    ) -> Result<Vec<ActivityLine>, Error> {
        let limit = limit.min(self.limits.page_rows);
        let bound = i64::try_from(limit).map_err(|_| Error::InvalidOperatorInput)?;
        let mut rows = Vec::new();
        for source in SOURCES
            .iter()
            .filter(|source| group.is_none_or(|group| source.group == group))
        {
            rows.extend(source.lines(&self.db, machine, before, bound)?);
        }
        rows.sort_unstable_by(newest_first);
        rows.truncate(limit);
        rows.into_iter()
            .map(|(sort, source, key, at, machine)| {
                Ok(ActivityLine {
                    cursor: ActivityCursor {
                        at: sort,
                        source,
                        key,
                    },
                    at,
                    machine,
                    what: self.what(source, key)?,
                })
            })
            .collect()
    }

    /// The marks the live pages compare to tell whether anything was stored.
    /// # Errors
    /// Reports database failures.
    pub fn activity_marks(&self) -> Result<ActivityMarks, Error> {
        Ok(self.db.query_row(
            concat!(
                "SELECT (SELECT coalesce(max(rowid),0) FROM receipts),",
                "(SELECT coalesce(max(rowid),0) FROM interpretations),",
                "(SELECT coalesce(max(id),0) FROM parked),",
                "(SELECT coalesce(max(value),0) FROM alert_counts WHERE kind='parked'),",
                "(SELECT coalesce(max(sequence),0) FROM outbox),",
                "(SELECT coalesce(max(sequence),0) FROM attempts),",
                "(SELECT coalesce(max(sequence),0) FROM machine_order_history),",
                "(SELECT coalesce(max(sequence),0) FROM audit)"
            ),
            [],
            |r| {
                Ok(ActivityMarks {
                    receipts: r.get(0)?,
                    readings: r.get(1)?,
                    parked: r.get(2)?,
                    open: r.get(3)?,
                    deliveries: r.get(4)?,
                    attempts: r.get(5)?,
                    orders: r.get(6)?,
                    audit: r.get(7)?,
                })
            },
        )?)
    }

    /// How many lines each group has in `day` (Unix seconds `from..until`),
    /// for `machine` when given, in [`ActivityGroup::ALL`]'s order. Each
    /// source counts a range of its time index.
    /// # Errors
    /// Reports database failures or a corrupt count.
    pub fn activity_counts(
        &self,
        machine: Option<&str>,
        day: (i64, i64),
    ) -> Result<Vec<(ActivityGroup, u64)>, Error> {
        ActivityGroup::ALL
            .into_iter()
            .map(|group| {
                let mut total = 0_i64;
                for source in SOURCES.iter().filter(|source| source.group == group) {
                    total = total
                        .checked_add(source.count(&self.db, machine, day)?)
                        .ok_or(Error::Corruption)?;
                }
                u64::try_from(total)
                    .map(|count| (group, count))
                    .map_err(|_| Error::Corruption)
            })
            .collect()
    }

    /// What the line from `source` row `key` says happened.
    fn what(&self, source: i64, key: i64) -> Result<Activity, Error> {
        match source {
            0 => {
                let evidence: Option<String> = self.db.query_row(
                    "SELECT (SELECT evidence FROM interpretations i WHERE i.receipt=r.id ORDER BY version DESC LIMIT 1) FROM receipts r WHERE r.rowid=?1",
                    [key],
                    |r| r.get(0),
                )?;
                Ok(Activity::Stored(reading(evidence)?))
            }
            1 => {
                let evidence: String =
                    self.db
                        .query_row("SELECT evidence FROM parked WHERE id=?1", [key], |r| {
                            r.get(0)
                        })?;
                Ok(Activity::Parked {
                    item: key,
                    details: serde_json::from_str::<crate::parked::ParkingEvidence>(&evidence)?
                        .details,
                })
            }
            2 => {
                let action: Option<String> = self.db.query_row(
                    "SELECT json_extract(resolution,'$.action') FROM parked WHERE id=?1",
                    [key],
                    |r| r.get(0),
                )?;
                Ok(Activity::Resolved {
                    item: key,
                    action: action.unwrap_or_default(),
                })
            }
            3 => self.attempt(key),
            4 => self.order(key),
            5 => {
                let evidence: String = self.db.query_row(
                    "SELECT evidence FROM audit WHERE sequence=?1",
                    [key],
                    |r| r.get(0),
                )?;
                let audit: crate::Audit = serde_json::from_str(&evidence)?;
                Ok(Activity::Changed {
                    actor: audit.actor.as_str().to_owned(),
                    action: audit.action.as_str().to_owned(),
                    target: audit
                        .targets
                        .first()
                        .map(|target| target.as_str().to_owned()),
                })
            }
            _ => Err(Error::Corruption),
        }
    }

    fn attempt(&self, key: i64) -> Result<Activity, Error> {
        let (outcome, lab_app, order, evidence): (String, String, Option<String>, Option<String>) =
            self.db.query_row(
                concat!(
                    "SELECT a.outcome,o.destination,o.order_id,",
                    "(SELECT evidence FROM interpretations i WHERE i.receipt=o.receipt AND i.version=o.version) ",
                    "FROM attempts a JOIN outbox o ON o.id=a.delivery WHERE a.sequence=?1"
                ),
                [key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        Ok(Activity::Attempt {
            lab_app,
            outcome: attempt_outcome(&outcome)?,
            carries: match order {
                Some(order) => Carries::OrderMessage { order },
                None => Carries::Results(reading(evidence)?),
            },
        })
    }

    fn order(&self, key: i64) -> Result<Activity, Error> {
        /// The fields of a stored order snapshot the feed shows.
        #[derive(Deserialize)]
        struct Snapshot {
            key: crate::MachineOrderKey,
            state: OrderState,
            #[serde(default)]
            items: Vec<serde::de::IgnoredAny>,
        }
        let evidence: String = self.db.query_row(
            "SELECT evidence FROM machine_order_history WHERE sequence=?1",
            [key],
            |r| r.get(0),
        )?;
        let snapshot: Snapshot = serde_json::from_str(&evidence)?;
        let sample: Option<String> = self
            .db
            .query_row(
                "SELECT sample FROM orders WHERE destination=?1 AND id=?2",
                params![
                    snapshot.key.destination.as_str(),
                    snapshot.key.order.as_str()
                ],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Activity::Order {
            order: snapshot.key.order.as_str().to_owned(),
            sample,
            // Orders reserved before items were recorded hold none.
            tests: (!snapshot.items.is_empty()).then_some(snapshot.items.len()),
            state: snapshot.state,
        })
    }
}

/// What a stored reading produced; none when there is no reading.
fn reading(evidence: Option<String>) -> Result<Option<RecentOutcome>, Error> {
    evidence
        .map(|evidence| serde_json::from_str(&evidence).map(|reading| RecentOutcome::of(&reading)))
        .transpose()
        .map_err(Error::from)
}

/// Reads an attempt's stored outcome, as `crate::Attempt` writes it.
fn attempt_outcome(stored: &str) -> Result<AttemptOutcome, Error> {
    let value: serde_json::Value = serde_json::from_str(stored)?;
    if let Some(word) = value.as_str() {
        return match word {
            "Done" => Ok(AttemptOutcome::Delivered),
            "uncertain (interrupted)" => Ok(AttemptOutcome::Interrupted),
            _ => Err(Error::Corruption),
        };
    }
    let object = value.as_object().ok_or(Error::Corruption)?;
    if let Some(at) = object.get("RetryAt") {
        return at
            .as_i64()
            .map(AttemptOutcome::RetryAt)
            .ok_or(Error::Corruption);
    }
    [
        ("Parked", AttemptOutcome::Rejected),
        ("Paused", AttemptOutcome::Paused),
        ("Conflict", AttemptOutcome::Conflict),
    ]
    .into_iter()
    .find_map(|(name, outcome)| object.contains_key(name).then_some(outcome))
    .ok_or(Error::Corruption)
}
