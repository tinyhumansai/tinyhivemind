//! Stable directory inputs and entries.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::trace::TopicId;

/// Weight above which a `(agent, topic)` estimate stops growing.
///
/// A long transcript deposits without bound, and an unbounded weight would let
/// one member's early lead become unreachable — the premature convergence
/// MAX–MIN Ant System clamps against. Saturating here keeps the fold total and
/// keeps a late specialist able to catch up.
pub const WEIGHT_CEILING: i64 = 1_000_000;

/// How the directory weighs what the transcript says about who knows what.
///
/// Every field is fixed point. `specialisation`, `credibility`, `prior` and
/// `discredit` are tenths; `floor` is a weight in thousandths.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryPolicy {
    /// Sequence distance at which a deposit's contribution halves.
    pub half_life: u32,
    /// Weight on a member's own grounded deposits, in tenths.
    pub specialisation: u16,
    /// Weight on other members' citations of them, in tenths.
    pub credibility: u16,
    /// Weight on the host's declared affinity, in tenths.
    pub prior: u16,
    /// What one objection costs, in tenths of one citation.
    pub discredit: u16,
    /// How many sequences back a deposit still counts.
    pub window: u32,
    /// Weight at or above which a member is said to *know* a topic.
    pub floor: i64,
}

impl DirectoryPolicy {
    /// A conservative default, pending the benchmark arm that scores it.
    ///
    /// Specialisation outweighs credibility because a deposit is a fact the
    /// member actually stated and a citation is a second member's opinion
    /// about it. The host's prior is a third of credibility because it is a
    /// *diffuse* cue in Hollingshead's sense — a role label rather than
    /// observed experience — and the whole point of folding a directory is
    /// that specific cues should displace diffuse ones as shared history
    /// accumulates.
    pub const DEFAULT: Self = Self {
        half_life: 20,
        specialisation: 30,
        credibility: 20,
        prior: 10,
        discredit: 20,
        window: 30,
        floor: 1_000,
    };
}

impl Default for DirectoryPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the transcript says one member knows about one topic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryEntry {
    /// Canonical agent id.
    pub agent_id: String,
    /// The topic this estimate is about.
    pub topic: TopicId,
    /// Decayed weight of this member's own grounded deposits on the topic.
    pub specialisation: i64,
    /// Decayed weight of other members' citations of them, net of objections
    /// and never below zero.
    pub credibility: i64,
    /// The combined estimate, clamped to `0..=WEIGHT_CEILING`.
    ///
    /// Zero when the member deferred the topic in window: a member saying
    /// "not mine" outranks anything the fold inferred about them.
    pub weight: i64,
}

/// Who knows what, folded from one transcript.
///
/// Wegner's directory: the group's memory is the index of who holds what, not
/// the contents. Entries are in `(topic, agent_id)` order, and only holders
/// appear — a pair every term scored zero on is dropped rather than carried as
/// a zero.
///
/// The field is private because the ordering is load-bearing for [`top`] and
/// [`lines`]; [`entries`] hands the whole slice back for a caller that wants
/// to audit the estimate, which the benchmark's circularity check does.
///
/// [`top`]: Directory::top
/// [`lines`]: Directory::lines
/// [`entries`]: Directory::entries
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Directory {
    entries: Vec<DirectoryEntry>,
}

/// Decode a directory, restoring the ordering its queries depend on.
///
/// The wire form is the entry array and nothing else — the same one
/// `#[serde(transparent)]` writes — but a decoded array is whatever the sender
/// wrote, and [`top`], [`lines`] and [`topics`] all read the entries as being
/// in `(topic, agent_id)` order. `topics` in particular compares each entry
/// with the previous one, so an unsorted array would report a topic twice.
/// Sorting here rather than trusting the sender is what keeps that invariant a
/// property of the type. A repeated `(topic, agent_id)` is rejected instead of
/// sorted: two weights for one pair is not a directory the fold could have
/// produced, and silently keeping either one would be a guess. An entry whose
/// `weight` falls outside `0..=WEIGHT_CEILING` is rejected the same way — the
/// fold never emits one, so a decoded value out there is not a directory the
/// fold could have produced either, and [`top`], [`top_among`], [`knows`] and
/// [`lines`] all read `weight` back unclamped.
///
/// [`top`]: Directory::top
/// [`top_among`]: Directory::top_among
/// [`knows`]: Directory::knows
/// [`lines`]: Directory::lines
/// [`topics`]: Directory::topics
impl<'de> Deserialize<'de> for Directory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut entries = Vec::<DirectoryEntry>::deserialize(deserializer)?;
        if let Some(entry) = entries
            .iter()
            .find(|entry| !(0..=WEIGHT_CEILING).contains(&entry.weight))
        {
            return Err(D::Error::custom(format!(
                "directory weight for {} on #{} is out of range",
                entry.agent_id, entry.topic
            )));
        }
        entries.sort_by(|left, right| {
            left.topic
                .cmp(&right.topic)
                .then_with(|| left.agent_id.cmp(&right.agent_id))
        });
        if let Some(pair) = entries
            .windows(2)
            .find(|pair| pair[0].topic == pair[1].topic && pair[0].agent_id == pair[1].agent_id)
        {
            return Err(D::Error::custom(format!(
                "duplicate directory entry for {} on #{}",
                pair[0].agent_id, pair[0].topic
            )));
        }
        Ok(Self { entries })
    }
}

impl Directory {
    /// Build a directory from entries already in `(topic, agent_id)` order.
    pub(crate) fn new(entries: Vec<DirectoryEntry>) -> Self {
        Self { entries }
    }

    /// Every entry, in `(topic, agent_id)` order.
    ///
    /// Public so a harness can rank-correlate directory weight against speech
    /// share. A directory that only reproduces who talked has learned nothing,
    /// and that has to be measurable from outside the crate.
    #[must_use]
    pub fn entries(&self) -> &[DirectoryEntry] {
        &self.entries
    }

    /// Every topic some member holds, in topic order, without repeats.
    #[must_use]
    pub fn topics(&self) -> Vec<&TopicId> {
        let mut topics: Vec<&TopicId> = Vec::new();
        for entry in &self.entries {
            if topics.last() != Some(&&entry.topic) {
                topics.push(&entry.topic);
            }
        }
        topics
    }

    /// This member's weight on a topic, or zero when it holds no entry.
    #[must_use]
    pub fn weight(&self, agent_id: &str, topic: &TopicId) -> i64 {
        self.entries
            .iter()
            .find(|entry| entry.agent_id == agent_id && &entry.topic == topic)
            .map_or(0, |entry| entry.weight)
    }

    /// Whether this member's weight reaches `policy.floor`.
    #[must_use]
    pub fn knows(&self, agent_id: &str, topic: &TopicId, policy: &DirectoryPolicy) -> bool {
        self.weight(agent_id, topic) >= policy.floor
    }

    /// The highest-weighted holder of a topic, ties broken by agent id.
    #[must_use]
    pub fn top(&self, topic: &TopicId) -> Option<&DirectoryEntry> {
        self.entries
            .iter()
            .filter(|entry| &entry.topic == topic)
            .reduce(|held, next| {
                if next.weight > held.weight {
                    next
                } else {
                    held
                }
            })
    }

    /// The highest-weighted holder of a topic among the given members.
    ///
    /// Ties break by the order the members were given, which is desk order, so
    /// the answer is deterministic for a given roster and transcript. A member
    /// with no entry for the topic is not a candidate, so a room where nobody
    /// has deposited anything on it returns `None` rather than its first
    /// member.
    #[must_use]
    pub fn top_among<'a>(&self, topic: &TopicId, members: &[&'a str]) -> Option<&'a str> {
        members
            .iter()
            .filter_map(|member| {
                self.entries
                    .iter()
                    .find(|entry| entry.agent_id == *member && &entry.topic == topic)
                    .map(|entry| (*member, entry.weight))
            })
            .reduce(|held, next| if next.1 > held.1 { next } else { held })
            .map(|(member, _)| member)
    }

    /// Render one line per topic, holders in descending weight order.
    ///
    /// ```text
    /// #pool: archivist 1420 (spec 900, cred 520) · critic 300 (spec 300, cred 0)
    /// ```
    ///
    /// This exists so a host can paste the directory into a prompt — Stasser's
    /// public expert-role assignment, which raises unique-item sampling in a
    /// hidden profile. Nothing in this crate renders it, and a host that does
    /// should know it is also rendering the host's own prior as if the room
    /// had earned it.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.topics()
            .into_iter()
            .map(|topic| {
                let mut holders: Vec<&DirectoryEntry> = self
                    .entries
                    .iter()
                    .filter(|entry| &entry.topic == topic)
                    .collect();
                holders.sort_by(|left, right| {
                    right
                        .weight
                        .cmp(&left.weight)
                        .then_with(|| left.agent_id.cmp(&right.agent_id))
                });
                let holders: Vec<String> = holders
                    .into_iter()
                    .map(|entry| {
                        format!(
                            "{} {} (spec {}, cred {})",
                            entry.agent_id, entry.weight, entry.specialisation, entry.credibility,
                        )
                    })
                    .collect();
                format!("#{topic}: {}", holders.join(" · "))
            })
            .collect()
    }
}
