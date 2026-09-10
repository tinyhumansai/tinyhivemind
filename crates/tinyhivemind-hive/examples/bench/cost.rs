//! What an arm costs in tokens, and how long a host waits for it.
//!
//! Every other accounting in this harness is in units only this harness uses:
//! a turn, a round, a `cost_unit`, a nanosecond spent inside `step`. Those are
//! the right units for asking what the *library* costs, and
//! [`crate::metrics::Aggregate::nanos_per_step`] answers that question well.
//! They are the wrong units for asking what a *system* costs, and reading them
//! as though they were is the single largest source of misreading in the
//! recorded results: `vote` reports `0` ns/step and an infinite `episodes/s`,
//! because a poll never calls the library at all. Nobody deploying a room cares
//! that the state machine is free. They care what the seats cost and how long
//! the answer takes.
//!
//! This module prices that. It converts the shape an episode actually ran —
//! how many rounds deep, how wide each round was, and how much transcript each
//! round's turns had to read — into tokens and milliseconds, so that quality,
//! speed, throughput, concurrency and token cost all come out of one
//! simulation in units that can be compared across every arm and against a
//! live run.
//!
//! # It is a model, and it says so
//!
//! Four constants stand between a round shape and a millisecond, and none of
//! them is measured here. They are printed in the run header with the flags
//! that set them, and `--calibrate` measures them from real traffic against
//! `--api-base` so the default point can be argued with rather than inherited.
//! A finding that survives only at one setting is a finding about this file;
//! sweep the constants and see.
//!
//! # What it deliberately does not model
//!
//! Queueing. Every round is priced as though the host had a seat free for
//! every turn the round authorized, so [`Cost::concurrency`] is the width the
//! *protocol* asked for rather than the width a particular deployment could
//! afford. A host with fewer seats than `round_width` pays more wall clock
//! than this reports, in proportion to how far short it falls. Charging for a
//! seat pool would make the arms incomparable across scales, which is the axis
//! the grid exists to walk.

/// The four constants that turn a round shape into tokens and milliseconds.
///
/// Defaults are a mid-sized hosted model serving a short structured turn, and
/// are the point every table in this harness reports at unless a flag moves
/// them. See [`CostModel::header`] for how a run declares the point it used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CostModel {
    /// Prompt tokens one transcript row contributes to a turn that reads it.
    ///
    /// A row here is one appended message — a `!propose`, an `!evidence`
    /// deposit, an operator brief. The harness counts rows everywhere else
    /// (`SESSION_WINDOW`, `PIN_SCAN`, [`crate::context`]), so pricing per row
    /// keeps one unit across the whole benchmark rather than introducing a
    /// second one that would have to be kept consistent with the first.
    pub(crate) tokens_per_row: u32,
    /// Completion tokens one turn writes.
    pub(crate) tokens_per_turn: u32,
    /// Prompt tokens charged to every turn before it reads a single row: the
    /// system prompt, the desk description, the task brief and the protocol
    /// instructions.
    ///
    /// It matters more than it looks. A one-turn arm pays it once and a
    /// deliberation pays it on every turn, so an arm's token cost is not
    /// proportional to its turns — which is exactly the effect a `cost/ep`
    /// column counting turns could never show.
    pub(crate) prompt_base: u32,
    /// Milliseconds before a seat's first completion token arrives.
    pub(crate) ttft_ms: u32,
    /// Completion tokens a seat decodes per second, once it has started.
    ///
    /// Never zero: [`CostModel::set`] refuses a zero, because it divides.
    pub(crate) decode_tok_s: u32,
}

impl CostModel {
    /// The point every table reports at unless a flag moves it.
    pub(crate) const DEFAULT: Self = Self {
        tokens_per_row: 42,
        tokens_per_turn: 180,
        prompt_base: 600,
        ttft_ms: 450,
        decode_tok_s: 85,
    };

    /// Apply one `--<name> <value>` cost-model flag.
    ///
    /// Returns whether `flag` named one of them, so the parser can tell a flag
    /// this module handles from one nobody does.
    ///
    /// # Errors
    ///
    /// Returns a message when the value is missing, unparsable, or — for
    /// `--decode-rate` — zero, which would divide.
    pub(crate) fn set(
        &mut self,
        flag: &str,
        args: &mut impl Iterator<Item = String>,
    ) -> Result<bool, String> {
        let field = match flag {
            "--tokens-per-row" => &mut self.tokens_per_row,
            "--tokens-per-turn" => &mut self.tokens_per_turn,
            "--prompt-base" => &mut self.prompt_base,
            "--ttft" => &mut self.ttft_ms,
            "--decode-rate" => &mut self.decode_tok_s,
            _ => return Ok(false),
        };
        // Parsed strictly rather than defaulted. A missing or nonnumeric value
        // silently taken as zero would report a benchmark nobody asked for --
        // free tokens, or an instant answer -- under the heading the operator
        // typed, and would additionally swallow whatever flag came next.
        let raw = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        let value = raw
            .parse::<u32>()
            .map_err(|_| format!("{flag} takes a whole number, not {raw}"))?;
        if flag == "--decode-rate" && value == 0 {
            return Err("--decode-rate must be above zero".to_owned());
        }
        *field = value;
        Ok(true)
    }

    /// The header line a run prints so a reader sees the model rather than
    /// inheriting it, with the flags that would reproduce it.
    pub(crate) fn header(&self) -> String {
        format!(
            "cost model: {} tok/row  {} tok/turn  {} tok prompt base  {} ms TTFT  {} tok/s decode\n\
             \x20           (--tokens-per-row {} --tokens-per-turn {} --prompt-base {} --ttft {} --decode-rate {})",
            self.tokens_per_row,
            self.tokens_per_turn,
            self.prompt_base,
            self.ttft_ms,
            self.decode_tok_s,
            self.tokens_per_row,
            self.tokens_per_turn,
            self.prompt_base,
            self.ttft_ms,
            self.decode_tok_s,
        )
    }

    /// What one turn reading `rows` of transcript costs in prompt tokens.
    fn prompt_tokens(&self, rows: u32) -> u64 {
        u64::from(self.prompt_base) + u64::from(rows) * u64::from(self.tokens_per_row)
    }

    /// How long one turn takes, in milliseconds.
    ///
    /// Independent of how much prompt it read: prefill is overlapped with the
    /// time-to-first-token every hosted model already charges, and modelling a
    /// separate prefill rate would add a fifth constant that no `--calibrate`
    /// run can separate from the fourth.
    fn turn_ms(&self) -> u64 {
        u64::from(self.ttft_ms)
            + u64::from(self.tokens_per_turn) * 1_000 / u64::from(self.decode_tok_s)
    }

    /// Price one episode's round shape.
    ///
    /// Turns inside one round are authorized against the same transcript and
    /// cannot read each other, so a host with a seat free for each of them
    /// waits once for the whole round — the wall clock of a round is one
    /// turn's, however wide it is. That is the entire reason depth and width
    /// are counted separately, and it is what makes `concurrency` a column
    /// worth printing rather than a restatement of `turns/ep`.
    pub(crate) fn price(&self, shape: &[RoundShape]) -> Cost {
        let mut cost = Cost::ZERO;
        for round in shape {
            let width = u64::try_from(round.rows.len()).unwrap_or(u64::MAX);
            cost.turns = cost.turns.saturating_add(width);
            cost.rounds = cost.rounds.saturating_add(1);
            // Priced per turn rather than `rows.first() * width`: turns in
            // one round need not have read the same number of rows. A blind
            // turn withholds its peers' rows from this same round, and an
            // exchange round asks members whose own history differs, so the
            // uniform case (every turn reading the same count) is the common
            // one rather than the only one.
            for rows in &round.rows {
                cost.prompt = cost.prompt.saturating_add(self.prompt_tokens(*rows));
            }
            cost.completion = cost
                .completion
                .saturating_add(u64::from(self.tokens_per_turn) * width);
            cost.wall_ms = cost.wall_ms.saturating_add(self.turn_ms());
        }
        cost
    }
}

impl Default for CostModel {
    /// [`CostModel::DEFAULT`], so an [`crate::metrics::Aggregate`] built by
    /// `Default` is priced at the same point as one built by
    /// [`crate::metrics::Aggregate::priced_at`].
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One round of an episode, as the thing that gets priced.
///
/// Recorded by the driver rather than reconstructed afterwards: an arm that
/// opens an off-floor exchange round, or a federation that runs every desk's
/// round in flight at once, has a shape no `(turns, rounds)` pair can express,
/// and guessing one from the pair is how a benchmark starts reporting a
/// protocol it never ran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RoundShape {
    /// Transcript rows each turn in this round actually read, one entry per
    /// turn the round authorized.
    ///
    /// Not necessarily uniform: a blind turn withholds its peers' rows from
    /// this same round, so two turns authorized together can read different
    /// counts, and an exchange round asks members whose own prior history
    /// differs from each other's. [`RoundShape::uniform`] is the shorthand for
    /// the common case where every turn does read the same count.
    pub(crate) rows: Vec<u32>,
}

impl RoundShape {
    /// `turns` turns, each reading the same `rows` — the shape every turn in
    /// a round takes when nothing makes one of them differ from the rest: a
    /// synthetic one-round arm like [`crate::arms::blind_shape`], or a test
    /// fixture that is not asking about the non-uniform case.
    pub(crate) fn uniform(rows: u32, turns: u32) -> Self {
        Self {
            rows: vec![rows; usize::try_from(turns).unwrap_or(usize::MAX)],
        }
    }
}

/// What an episode cost, in units a host recognises.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cost {
    /// Prompt tokens across every turn.
    pub(crate) prompt: u64,
    /// Completion tokens across every turn.
    pub(crate) completion: u64,
    /// Milliseconds a host waited, start to decision.
    pub(crate) wall_ms: u64,
    /// Turns taken: the episode's width.
    pub(crate) turns: u64,
    /// Rounds taken: the episode's depth, and what the wall clock is a sum
    /// over.
    pub(crate) rounds: u64,
}

impl Cost {
    /// An episode that has not run yet.
    pub(crate) const ZERO: Self = Self {
        prompt: 0,
        completion: 0,
        wall_ms: 0,
        turns: 0,
        rounds: 0,
    };

    /// Every token, prompt and completion alike.
    ///
    /// Summed rather than reported separately in the headline table because
    /// the ratio between them is a property of [`CostModel`] and not of any
    /// arm; the JSON output carries both for anyone who wants to reprice a
    /// recorded run against different constants.
    pub(crate) fn tokens(&self) -> u64 {
        self.prompt.saturating_add(self.completion)
    }

    /// Fold another episode's cost into a running sample total.
    ///
    /// Associative and commutative on every field, which is what lets the
    /// per-room loops run across `--jobs` threads and still print the same
    /// bytes — the same property [`crate::metrics::Aggregate::merge`] relies
    /// on, tested the same way.
    pub(crate) fn merge(&mut self, other: Self) {
        self.prompt = self.prompt.saturating_add(other.prompt);
        self.completion = self.completion.saturating_add(other.completion);
        self.wall_ms = self.wall_ms.saturating_add(other.wall_ms);
        self.turns = self.turns.saturating_add(other.turns);
        self.rounds = self.rounds.saturating_add(other.rounds);
    }

    /// Mean turns in flight: how much of the room's width the arm actually
    /// used.
    ///
    /// `1.0` is a strictly sequential protocol, whatever its `round_width`.
    /// The ceiling is the room's size, reached only by an arm that authorizes
    /// every member on every round — `vote`, which is one round of everybody.
    pub(crate) fn concurrency(&self) -> f64 {
        if self.rounds == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.turns as f64 / self.rounds as f64
        }
    }

    /// Seconds of wall clock across the sample.
    pub(crate) fn wall_secs(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        {
            self.wall_ms as f64 / 1_000.0
        }
    }

    /// Completion and prompt tokens per second of wall clock.
    ///
    /// The rate a deployment's seats are actually turning over, which is what
    /// a capacity plan is written against. An arm can be cheap per episode and
    /// still be the one that saturates a provider's rate limit, because it
    /// spends its tokens in a burst; this column is where that shows.
    pub(crate) fn tokens_per_second(&self) -> f64 {
        let secs = self.wall_secs();
        if secs <= 0.0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.tokens() as f64 / secs
        }
    }

    /// Episodes one seat pool finishes per hour, at this mean latency.
    pub(crate) fn episodes_per_hour(&self, episodes: u32) -> f64 {
        let secs = self.wall_secs();
        if secs <= 0.0 || episodes == 0 {
            return 0.0;
        }
        f64::from(episodes) * 3_600.0 / secs
    }

    /// Mean milliseconds per episode across `episodes` of them.
    pub(crate) fn mean_ms(&self, episodes: u32) -> f64 {
        if episodes == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.wall_ms as f64 / f64::from(episodes)
        }
    }

    /// Mean tokens per episode across `episodes` of them.
    pub(crate) fn mean_tokens(&self, episodes: u32) -> f64 {
        if episodes == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.tokens() as f64 / f64::from(episodes)
        }
    }
}

#[cfg(test)]
mod test;
