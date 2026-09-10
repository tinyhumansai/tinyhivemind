//! The four axes the grid walks, and what a point on each one means.
//!
//! Each axis is a named, ordered, small set of points rather than a free
//! number, and that is the whole design. Before the grid existed the same
//! four questions were asked by five separate sweep modes, each with its own
//! flags, its own table shape and its own markdown file — so an operator could
//! learn what a bigger room cost, and separately what a harder problem cost,
//! and never what the two cost together, because no two of those tables shared
//! a column. Naming the points is what makes a cell reproducible in a sentence
//! (`topic=hidden scale=9 complexity=4 concurrency=2`) and what lets one table
//! hold the whole cross product.

use crate::sim::Expertise;

/// What kind of problem the room is deciding.
///
/// Not the *subject* of the decision — every room here decides between
/// abstract options — but the shape of who knows what, which is the only thing
/// about a topic that any of these protocols can respond to. A room where
/// everybody is equally informed and a room where one member alone holds the
/// deciding fact are different problems in the sense that matters: the first
/// rewards aggregating, and the second rewards *finding*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Topic {
    /// Everybody draws independent noise around the same truth. Pooling is the
    /// whole job, and a poll is a strong control.
    Uniform,
    /// One member holds each of a few options far more tightly than anybody
    /// else. Routing to the right member is worth something, which is the
    /// premise the responder ladder is built on.
    Expert,
    /// A decoy sits above every member's own argmax except one, who alone
    /// holds the fact that rules it out. Averaging opinions cannot fix it —
    /// the room has to let the one member who knows actually say so.
    Hidden,
}

impl Topic {
    /// Every point on the axis, in the order a table prints them: easiest for
    /// a poll to solve first, hardest last.
    pub(crate) const ALL: [Self; 3] = [Self::Uniform, Self::Expert, Self::Hidden];

    /// Parse one `--topic` name.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "uniform" => Some(Self::Uniform),
            "expert" => Some(Self::Expert),
            "hidden" => Some(Self::Hidden),
            _ => None,
        }
    }

    /// The name `--topic` takes and the table prints.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Expert => "expert",
            Self::Hidden => "hidden",
        }
    }

    /// How this topic distributes private evaluations across a room of
    /// `agents`.
    ///
    /// `Expert` scales its specialist count with the room rather than fixing
    /// it, so the *share* of the room holding a specialty is what stays
    /// constant as the scale axis moves. Fixing the count instead would make
    /// the topic quietly easier at every larger scale, and the grid would
    /// report that as a property of scale.
    pub(crate) fn expertise(self, agents: usize) -> Expertise {
        match self {
            Self::Uniform => Expertise::Uniform,
            Self::Expert => Expertise::Specialists {
                count: (agents / 3).max(1),
            },
            Self::Hidden => Expertise::HiddenProfile,
        }
    }

    /// The evaluation noise this topic wants, given the complexity level's.
    ///
    /// `Hidden` overrides it. At the single-room default of ±90 the gap a
    /// planted decoy opens is swamped: a lay member's argmax lands on the
    /// truth often enough that a poll solves the profile by accident, and an
    /// arm that beats a control which already wins is measuring nothing. The
    /// same argument `HIDDEN_NOISE` is documented under in `cli/mod.rs`, and
    /// the same value, so a `--topic hidden` cell and a `--hidden-profile`
    /// run agree.
    pub(crate) fn noise(self, level_noise: u32) -> u32 {
        match self {
            Self::Hidden => 50,
            Self::Uniform | Self::Expert => level_noise,
        }
    }
}

/// How hard the call is, on a five-point scale.
///
/// One ordinal knob standing for two correlated ones — how many options are on
/// offer, and how noisy each member's read of them is — because they are not
/// independently interesting and sweeping them separately produces a
/// twenty-five-cell table whose diagonal is the only part anybody reads. Level
/// 3 is the harness's own historical default (4 options, ±90), so a grid cell
/// at level 3 reproduces the single-room comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Complexity(pub(crate) u8);

impl Complexity {
    /// Every level, easiest first.
    pub(crate) const ALL: [Self; 5] = [Self(1), Self(2), Self(3), Self(4), Self(5)];
    /// The level that reproduces the single-room comparison.
    pub(crate) const DEFAULT: Self = Self(3);

    /// Options on offer at this level.
    pub(crate) fn topics(self) -> usize {
        match self.0 {
            1 => 2,
            2 => 3,
            3 => 4,
            4 => 6,
            _ => 8,
        }
    }

    /// Half-width of the error on each private evaluation at this level.
    pub(crate) fn noise(self) -> u32 {
        match self.0 {
            1 => 40,
            2 => 70,
            3 => 90,
            4 => 120,
            _ => 150,
        }
    }

    /// Parse one `--complexity` level, refusing anything off the scale.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw.trim().parse::<u8>() {
            Ok(level @ 1..=5) => Some(Self(level)),
            _ => None,
        }
    }
}

/// One point of the cross product: a room shape and the width it deliberates
/// at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    /// Who knows what.
    pub(crate) topic: Topic,
    /// Members in the room.
    pub(crate) scale: usize,
    /// How hard the call is.
    pub(crate) complexity: Complexity,
    /// Turns one round may authorize at once.
    pub(crate) concurrency: u32,
}

impl Cell {
    /// The one-line name that reproduces this cell from the command line.
    pub(crate) fn label(&self) -> String {
        format!(
            "topic={} scale={} complexity={} concurrency={}",
            self.topic.name(),
            self.scale,
            self.complexity.0,
            self.concurrency,
        )
    }

    /// The four axis values as table cells, in axis order.
    ///
    /// Trailing gap included, so an arm name appended to this reads as its own
    /// column rather than running into the concurrency figure beside it.
    pub(crate) fn columns(&self) -> String {
        format!(
            "{:<9}{:>6}{:>6}{:>6}  ",
            self.topic.name(),
            self.scale,
            self.complexity.0,
            self.concurrency,
        )
    }
}

/// The four axes a run walks, each as the list of points asked for.
#[derive(Clone, Debug)]
pub(crate) struct Axes {
    /// Problem shapes.
    pub(crate) topics: Vec<Topic>,
    /// Room sizes.
    pub(crate) scales: Vec<usize>,
    /// Difficulty levels.
    pub(crate) complexities: Vec<Complexity>,
    /// Round widths.
    pub(crate) concurrencies: Vec<u32>,
}

impl Axes {
    /// A single cell reproducing the single-room comparison: everybody equally
    /// informed, five members, the historical difficulty, strictly sequential.
    ///
    /// A bare `--grid` therefore prints one small table rather than sixty, and
    /// each axis is widened by naming it. That default matters more than it
    /// looks: a benchmark whose cheapest invocation is a sixty-cell run is one
    /// nobody runs before pushing, and a benchmark nobody runs stops being
    /// true.
    pub(crate) fn point() -> Self {
        Self {
            topics: vec![Topic::Uniform],
            scales: vec![5],
            complexities: vec![Complexity::DEFAULT],
            concurrencies: vec![1],
        }
    }

    /// Every cell, in printing order: topic slowest-moving, concurrency
    /// fastest.
    ///
    /// The order is the reading order of the table, and it is chosen so that
    /// adjacent rows differ in one axis — the comparison a reader makes
    /// without meaning to.
    pub(crate) fn cells(&self) -> Vec<Cell> {
        let mut cells = Vec::new();
        for topic in &self.topics {
            for scale in &self.scales {
                for complexity in &self.complexities {
                    for concurrency in &self.concurrencies {
                        cells.push(Cell {
                            topic: *topic,
                            scale: *scale,
                            complexity: *complexity,
                            concurrency: *concurrency,
                        });
                    }
                }
            }
        }
        cells
    }

    /// Apply one grid axis flag.
    ///
    /// Returns whether `flag` named an axis, so the parser can tell a flag
    /// this module owns from one nobody does.
    ///
    /// # Errors
    ///
    /// Returns a message when the list is missing, empty, or holds a value
    /// that is not a point on that axis. Refused rather than skipped: a
    /// misspelled topic quietly dropped would run a *smaller* grid than the
    /// operator asked for and print it under the heading they typed, which is
    /// the failure this harness refuses an unrecognised flag for one level up.
    pub(crate) fn set(
        &mut self,
        flag: &str,
        args: &mut impl Iterator<Item = String>,
    ) -> Result<bool, String> {
        if !matches!(
            flag,
            "--topic" | "--scale" | "--complexity" | "--concurrency"
        ) {
            return Ok(false);
        }
        let list = args
            .next()
            .ok_or_else(|| format!("{flag} takes a comma-separated list"))?;
        let parts: Vec<&str> = list.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err(format!("{flag} takes a comma-separated list, not {list:?}"));
        }
        // `all` on any axis takes every point that axis defines, which is
        // what the enumerated `ALL` constants are for. Spelling out
        // `uniform,expert,hidden` does the same thing; this exists so that
        // widening an axis does not require knowing what is on it, and so
        // that a new point added to an axis is picked up by a run that asked
        // for all of them rather than silently left out.
        if parts == ["all"] {
            match flag {
                "--topic" => self.topics = Topic::ALL.to_vec(),
                "--complexity" => self.complexities = Complexity::ALL.to_vec(),
                other => {
                    return Err(format!(
                        "{other} has no fixed set of points, so it takes numbers rather than `all`"
                    ));
                }
            }
            return Ok(true);
        }
        match flag {
            "--topic" => {
                self.topics = parts
                    .iter()
                    .map(|name| {
                        Topic::parse(name).ok_or_else(|| {
                            format!("--topic takes uniform, expert or hidden, not {name:?}")
                        })
                    })
                    .collect::<Result<_, _>>()?;
            }
            "--scale" => {
                self.scales = parts
                    .iter()
                    .map(|raw| {
                        raw.parse::<usize>()
                            .ok()
                            .filter(|size| *size >= 2)
                            .ok_or_else(|| {
                                format!("--scale takes room sizes of two or more, not {raw:?}")
                            })
                    })
                    .collect::<Result<_, _>>()?;
            }
            "--complexity" => {
                self.complexities = parts
                    .iter()
                    .map(|raw| {
                        Complexity::parse(raw)
                            .ok_or_else(|| format!("--complexity takes 1 to 5, not {raw:?}"))
                    })
                    .collect::<Result<_, _>>()?;
            }
            _ => {
                self.concurrencies = parts
                    .iter()
                    .map(|raw| {
                        raw.parse::<u32>()
                            .ok()
                            .filter(|width| *width >= 1)
                            .ok_or_else(|| {
                                format!("--concurrency takes round widths of one or more, not {raw:?}")
                            })
                    })
                    .collect::<Result<_, _>>()?;
            }
        }
        Ok(true)
    }
}
