//! An in-memory journal that is a real [`SessionLog`].
//!
//! The host owns the log; this is the smallest host log that obeys the
//! port's contract, so an offline run, a test and a host with nothing better
//! yet read it through exactly the projection a live host's journal is read
//! through. It needs nothing the `offline` feature pulls, so it is always
//! here. Two rules decide who may
//! read a row, and they are the ones a host follows too:
//!
//! - A desk row with `only_for` reaches its author and the seats it names.
//! - A row in a conversation reaches every seat in it: the author of the ask
//!   row it hangs off, and every seat that ask was for.

use std::sync::{Mutex, PoisonError};

use tinyhivemind_core::runtime::aside::Audience;
use tinyhivemind_core::runtime::{
    LogMessage, Sequence, SessionAuthor, SessionFuture, SessionLog, SessionPage,
};

/// One row of the journal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    /// The sequence the journal gave it.
    pub sequence: Sequence,
    /// Who it is attributed to: a seat id, `operator`, or `desk`.
    pub author: String,
    /// What it says, as rendered.
    pub body: String,
    /// The conversation it is in, by its ask row, or `None` on the desk.
    pub thread: Option<Sequence>,
    /// On the desk, the seats it reaches; empty reaches the desk.
    pub only_for: Vec<String>,
    /// The desk it is on, when that is not this journal's own: a host's log
    /// spans its channels, and a seat reads its others as context.
    pub desk: Option<String>,
}

/// An append-only journal for one desk, held in memory.
#[derive(Debug)]
pub struct MemoryLog {
    desk: String,
    /// The sequence the first row is given.
    first: u64,
    rows: Mutex<Vec<Row>>,
}

impl MemoryLog {
    /// An empty journal for `desk`, numbering its rows from one.
    #[must_use]
    pub fn new(desk: impl Into<String>) -> Self {
        Self::numbered_from(desk, Sequence(1))
    }

    /// An empty journal for `desk` whose first row is given `first`: a host
    /// numbers its log as it likes, and some number the first row zero.
    #[must_use]
    pub fn numbered_from(desk: impl Into<String>, first: Sequence) -> Self {
        Self {
            desk: desk.into(),
            first: first.0,
            rows: Mutex::new(Vec::new()),
        }
    }

    fn rows(&self) -> std::sync::MutexGuard<'_, Vec<Row>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Append a row to this journal's own desk and return the sequence it
    /// was given.
    pub fn append(
        &self,
        author: &str,
        body: &str,
        thread: Option<Sequence>,
        only_for: &[String],
    ) -> Sequence {
        self.append_row(None, author, body, thread, only_for)
    }

    /// Append a row to another desk of the same host, sequenced with the
    /// rest: one log holds every channel, as a host's does.
    pub fn append_to(
        &self,
        desk: &str,
        author: &str,
        body: &str,
        thread: Option<Sequence>,
        only_for: &[String],
    ) -> Sequence {
        self.append_row(Some(desk), author, body, thread, only_for)
    }

    fn append_row(
        &self,
        desk: Option<&str>,
        author: &str,
        body: &str,
        thread: Option<Sequence>,
        only_for: &[String],
    ) -> Sequence {
        let mut rows = self.rows();
        let sequence = Sequence(rows.last().map_or(self.first, |row| row.sequence.0 + 1));
        rows.push(Row {
            sequence,
            author: author.to_owned(),
            body: body.to_owned(),
            thread,
            only_for: only_for.to_vec(),
            desk: desk.map(str::to_owned),
        });
        sequence
    }

    /// The newest sequence, or `None` for an empty journal.
    #[must_use]
    pub fn latest(&self) -> Option<Sequence> {
        self.rows().last().map(|row| row.sequence)
    }

    /// What `seat` may read on the open desk above `after`, rendered; all of
    /// it for `None`.
    #[must_use]
    pub fn desk_since(&self, seat: &str, after: Option<Sequence>) -> Vec<String> {
        self.rows()
            .iter()
            .filter(|row| row.desk.is_none())
            .filter(|row| after.is_none_or(|after| row.sequence > after) && row.thread.is_none())
            .filter(|row| {
                row.only_for.is_empty()
                    || row.author == seat
                    || row.only_for.iter().any(|only| only == seat)
            })
            .map(render)
            .collect()
    }

    /// One conversation whole, rendered: the ask that rooted it and every row
    /// in it.
    #[must_use]
    pub fn thread(&self, root: Sequence) -> Vec<String> {
        self.thread_since(root, None)
    }

    /// One conversation above `after`, rendered; whole for `None`.
    #[must_use]
    pub fn thread_since(&self, root: Sequence, after: Option<Sequence>) -> Vec<String> {
        self.rows()
            .iter()
            .filter(|row| after.is_none_or(|after| row.sequence > after))
            .filter(|row| row.sequence == root || row.thread == Some(root))
            .map(render)
            .collect()
    }

    /// Every row, in order.
    #[must_use]
    pub fn all(&self) -> Vec<Row> {
        self.rows().clone()
    }

    /// Who may read `row`, by the two rules in the module docs.
    fn audience(rows: &[Row], row: &Row) -> Audience {
        let addressed: Vec<String> = match row.thread {
            Some(root) => rows
                .iter()
                .find(|candidate| candidate.sequence == root)
                .map(|ask| {
                    std::iter::once(ask.author.clone())
                        .chain(ask.only_for.iter().cloned())
                        .collect()
                })
                .unwrap_or_default(),
            None => row.only_for.clone(),
        };
        let mut members: Vec<String> = Vec::new();
        for id in addressed {
            if id != row.author && !members.contains(&id) {
                members.push(id);
            }
        }
        if members.is_empty() && row.thread.is_none() && row.only_for.is_empty() {
            Audience::Desk
        } else {
            Audience::Aside { members }
        }
    }

    fn author(id: &str) -> SessionAuthor {
        match id {
            "operator" => SessionAuthor::Operator,
            "desk" => SessionAuthor::System {
                kind: "desk".into(),
                label: "desk".into(),
            },
            seat => SessionAuthor::Agent {
                id: seat.into(),
                label: seat.into(),
            },
        }
    }
}

/// `@author: body`, as every reader of this journal sees a row.
fn render(row: &Row) -> String {
    format!("@{}: {}", row.author, row.body)
}

impl SessionLog for MemoryLog {
    fn read_before(&self, before: Option<Sequence>, limit: usize) -> SessionFuture<'_> {
        let rows = self.all();
        let older: Vec<&Row> = rows
            .iter()
            .rev()
            .filter(|row| before.is_none_or(|bound| row.sequence < bound))
            .collect();
        let taken = &older[..older.len().min(limit)];
        let next_before = if older.len() > taken.len() {
            taken.last().map(|row| row.sequence)
        } else {
            None
        };
        let messages = taken
            .iter()
            .map(|row| LogMessage {
                sequence: row.sequence,
                chat_id: Some(row.desk.clone().unwrap_or_else(|| self.desk.clone())),
                parent: row.thread,
                author: Self::author(&row.author),
                content: row.body.clone(),
                audience: Self::audience(&rows, row),
            })
            .collect();
        Box::pin(async move {
            Ok(SessionPage {
                messages,
                next_before,
            })
        })
    }
}

#[cfg(test)]
mod test;
