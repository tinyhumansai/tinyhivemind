# The biology of a shared medium

Working notes behind [`../../wiki/Further-reading.md`](../../wiki/Further-reading.md).
For each mechanism: where it comes from, the quantitative form where one exists,
and **what this workspace would have to represent to implement it**.

Equations marked *(canonical form)* are the published formulation reconstructed
from the literature rather than extracted from the primary source in one
sitting. The citation is exact; check the constants against the paper before
they become code.

## Stigmergy

Pierre-Paul Grassé, *Insectes Sociaux* 6:41–80 (1959), from termite nest repair:
workers deposit pellets quasi-randomly, the deposit raises the local stimulus,
the raised stimulus raises the deposit probability, and pillars grow and arch.
Coordination is "the stimulation of workers by the performance they have
achieved". No worker holds a blueprint and no worker addresses another. The
work in progress *is* the message.

**Sematectonic versus sign-based.** E. O. Wilson, *Sociobiology* (1975), named
Grassé's original case *sematectonic*: the stimulus is the state of the work
itself. A pheromone trail is *sign-based* — a dedicated marker with no
constructive function. The distinction is load-bearing. A sematectonic trace is
self-validating, because the artifact is the evidence; a marker is cheap, and
can be stale or wrong.

`tinyhivemind-hive` implements sign-based stigmergy only. A `!support #topic`
is a marker. Nothing in the trace grammar points at the state of the work, and
the [live hidden-profile experiment](../experiments/2026-09-01-live-hidden-profile.md)
records exactly the failure that predicts: markers accumulated on a decoy while
the refuting *fact* sat in the transcript, inert.

**Qualitative versus quantitative.** Theraulaz & Bonabeau, "A Brief History of
Stigmergy", *Artificial Life* 5(2):97–116 (1999). Qualitative stigmergy maps a
discrete configuration to a discrete action; quantitative stigmergy varies one
scalar and changes only the *probability* of the same action. `TraceKind` is
qualitative; `Salience` is quantitative. Having both is right, and unusual.

**The medium's own properties.** Francis Heylighen, "Stigmergy as a universal
coordination mechanism I & II", *Cognitive Systems Research* 38:4–13 and 50–59
(2016), decomposes it into agent, action, medium, trace, condition, and lists
the medium's design axes: persistence, decay rate, locality, modifiability, and
whether traces superpose. Those five are a checklist for the transcript, and
this workspace has answers for three of them.

### Deposit, evaporation, diffusion

The behavioural source is the double bridge — Goss, Aron, Deneubourg & Pasteels,
*Naturwissenschaften* 76:579–581 (1989); Deneubourg et al., *J. Insect Behavior*
3:159–168 (1990) — with the fitted choice rule *(canonical form)*

    p₁ = (k + m₁)ⁿ / ((k + m₁)ⁿ + (k + m₂)ⁿ)

where `mᵢ` counts ants that already took each branch, `n ≈ 2` (superlinear, so
symmetry breaks; `n = 1` never locks in), and `k ≈ 20` is the attraction
threshold — how much trail is needed before it beats intrinsic randomness. `k`
is the exploration floor.

The original experiment has **no evaporation term at all**. The short branch
wins because returning ants arrive sooner and reinforce it twice as fast.
Latency is itself a reinforcement signal, which is worth noticing in a system
whose only clock is a sequence number.

Evaporation arrives with the optimization literature — Dorigo, Maniezzo &
Colorni, *IEEE Trans. SMC-B* 26(1):29–41 (1996); Dorigo & Stützle, *Ant Colony
Optimization*, MIT Press (2004):

    τ ← (1 − ρ)·τ + Σₖ Δτᵏ,     Δτᵏ = Q / Lₖ  if ant k used the edge

Deposit is **quality-weighted** — `Q/Lₖ`, scaled by the cost of the solution the
ant actually found, not by the act of depositing. The transition rule
`p ∝ τ^α · η^β` splits social evidence (`α`) from private evidence (`β`), and
that single ratio is the exploitation/exploration dial.

Two variants are directly relevant. MAX–MIN Ant System (Stützle & Hoos, *Future
Generation Computer Systems* 16(8):889–914, 2000) clamps `τ ∈ [τ_min, τ_max]`,
guaranteeing no trace ever reaches probability 0 or 1 — a hard floor against
premature convergence. Ant Colony System (Dorigo & Gambardella, *IEEE Trans.
Evolutionary Computation* 1(1):53–66, 1997) adds a *local* update that
**decrements** the edge an ant just used, pushing concurrent ants apart within
one iteration. That is an anti-herding device, and it is the same shape as
`SPEAK_COST` raising the speaker's own threshold.

Evaporation buys three separable things, which argues for three constants
rather than one: **staleness** (a trace's information value decays at the
environment's rate of change), **escape from local optima** (without it the
first lucky path monopolizes probability mass — a stigmergic information
cascade), and **bounded memory** (decay turns a trail from an unbounded
cumulative count, dominated by history, into a rate, dominated by the present).

> **What this workspace would have to represent.** `salience` already decays by
> sequence rank with a `half_life`, which covers staleness. It has no floor and
> no ceiling — nothing corresponding to `τ_min`/`τ_max` — so a topic that falls
> behind can never recover attention, and there is no `Q/Lₖ`: a deposit's weight
> comes from its `TraceKind`, never from the measured quality of what it
> deposited. A sematectonic trace kind, pointing at the state of the work rather
> than asserting a position, is the larger gap.

## Quorum sensing

**Bacterial.** *Vibrio fischeri*: LuxI synthesizes an acyl-homoserine lactone
that diffuses freely; LuxR binds it; the complex activates the *lux* operon
including `luxI` itself, closing a positive autoregulatory loop. Engebrecht,
Nealson & Silverman, *Cell* 32:773–781 (1983); Fuqua, Winans & Greenberg,
*J. Bacteriol.* 176:269–275 (1994), which coined the term.

The minimal model *(canonical form; Dockery & Keener, Bull. Math. Biol.
63:95–116, 2001)* gives internal autoinducer `A` a basal rate, a Hill-shaped
autoinduced rate, linear degradation, and membrane exchange with an external
pool whose concentration scales with cell density `ρ`. For Hill coefficient
`n > 1` there are three steady states over a range of `ρ`, with a saddle-node at
each end — so the system is **bistable and hysteretic**: the density at which it
switches on exceeds the density at which it switches off. Williams et al.,
*Molecular Systems Biology* 4:234 (2008), show positive feedback on *signal
production* is the loop that matters.

Hysteresis is the point, not an artifact. A single sharp threshold chatters when
the signal sits near it; distinct on and off thresholds commit and stay
committed.

**Ants.** Pratt, Mallon, Sumpter & Franks, *Behav. Ecol. Sociobiol.* 52:117–127
(2002); Pratt, *Behavioral Ecology* 16(2):488–496 (2005). *Temnothorax*
house-hunting runs a two-phase recruitment switch:

- **Before quorum**, a scout performs **tandem runs** — slow, one recruit at a
  time, and the recruit learns the route and **independently evaluates the
  site**. Independence of judgement is preserved.
- **After quorum**, she switches to **social carrying** — roughly three times
  faster, but carried ants are passive and do not assess anything.

The quorum is sensed by *encounter rate* with nestmates at the site, not by
counting: a local, memory-free estimator of a global quantity.

**Speed against accuracy.** Franks et al., *Proc. R. Soc. B* 270:2457–2463
(2003): harassed colonies **lower** the threshold, emigrate faster, and accept
worse sites. Pratt & Sumpter, *PNAS* 103:15906–15910 (2006), make the threshold
the single tunable that trades time against P(best site).

**The quorum response function.** Sumpter & Pratt, "Quorum responses and
consensus decision making", *Phil. Trans. R. Soc. B* 364:743–753 (2009), eq. 4.1:

    pₓ = a + (m − a) · xᵏ / (Tᵏ + xᵏ)

with `x` already committed, `T` the threshold, `k` the steepness, `a` the
spontaneous non-social commitment probability. **A quorum response exists when
`k ≥ 2`** — sub-linear below `T`, super-linear above it. That sub-linear region
is an *active suppression of early social influence*: an anti-cascade filter
built into the response curve. Their worked figure: forty individuals at 66.7%
individual accuracy reach 83.3% group accuracy with a steep response (`k = 9`)
against 75.5% with a shallow one, at the cost of taking longer.

Marshall, Kurvers, Krause & Wolf, *eLife* 8:e40368 (2019), close it out: under
signal-detection theory **simple majority voting is frequently sub-optimal**,
and the optimal pooling rule for independent judgements is a quorum with a
threshold that is not 50%.

> **What this workspace would have to represent.** `QuorumPolicy` has `threshold`
> and `window` but only one threshold, so a room at the boundary can flap
> between `Quorum` and `Deliberating` as the window slides; distinct on and off
> thresholds would fix that. There is no `k`: support is counted, so the response
> is a step, and the sub-linear anti-cascade region does not exist. And the
> library cannot distinguish a tandem-run recruit — a member who re-derived the
> position — from a carried one. That distinction is [`0004`](../adr/0004-grounds-are-weighed-by-evidential-depth.md).

## Honeybee nest-site selection

Lindauer, *Z. vergl. Physiol.* 37:263–324 (1955); Seeley & Visscher, *Apidologie*
35:101–116 (2004); Seeley, *Honeybee Democracy*, Princeton (2010). Around three
to five hundred scouts choose one cavity from a dozen, and roughly 5% of bees
ever visit more than one site. **Nobody compares options.**

**The dance is an absolute, private estimate.** Seeley & Visscher, *J. Exp.
Biol.* 211:3691–3697 (2008): the number of waggle runs is proportional to the
scout's own assessment of site quality. Each scout broadcasts an absolute scalar
she derived alone. The comparison happens in the medium, not in any head.

**Dissent expires on its own.** Seeley & Visscher, "Consensus building during
nest-site selection in honey bee swarms: the expiration of dissent", *Behav.
Ecol. Sociobiol.* 54:511–520 (2003). On each successive return a scout dances
fewer runs — a measured mean decay of about **−15.7 waggle runs per return
trip** — until she stops and reverts to neutral. And 23 of 27 scouts dancing for
a losing site **stopped before ever following a dance for another site**.
Dissent dies out; it is not argued down.

Two consequences worth carrying. Support persists only if re-earned by fresh
visits — evaporation implemented in individual memory rather than in the
environment. And because the decay is roughly constant in *absolute* runs, a
site that started with many runs survives many more trips than a mediocre one:
decay is a **quality-proportional survival filter**, not a flat tax.

**Cross-inhibition.** Seeley, Visscher, Schlegel, Hogan, Franks & Marshall,
"Stop signals provide cross inhibition in collective decision-making by honeybee
swarms", *Science* 335:108–111 (2012). The stop signal is a ~350 Hz vibratory
pulse delivered by butting. The discovery is that it is **targeted**: a scout
committed to one site preferentially signals scouts dancing for *other* sites,
and the recipient ceases dancing. Its function is to break a deadlock between
two equally good sites, which decay plus recruitment alone cannot do.

**The equations.** Pais, Hogan, Schlegel, Franks, Leonard & Marshall, "A
mechanism for value-sensitive decision-making", *PLoS ONE* 8(9):e73216 (2013):

    dx_A/dt = α_A(v_A)·x_u − ρ_A·x_A − β·x_A·x_B + σẆ_A
    dx_B/dt = α_B(v_B)·x_u − ρ_B·x_B − β·x_B·x_A + σẆ_B
    x_u + x_A + x_B = 1

`x_A`, `x_B` are the population fractions committed to each site and `x_u` the
uncommitted pool; `α_i(v_i)` is the recruitment rate, **increasing in the site's
value `v_i`**; `ρ_i` is spontaneous abandonment — the dance-decay term; `β` is
the stop-signal rate.

The structure is what matters. Recruitment is bilinear in `x_u`, so you can only
recruit from the uncommitted. Inhibition is bilinear in `x_A·x_B`, so it
requires an encounter between opposed advocates. Decay is linear.

Results: for equal alternatives there is a **pitchfork bifurcation at
β* = 2ρ/μ**, with `μ` the mean alternative value — below it the symmetric
deadlock is stable and the swarm splits. Above it, deadlock destabilizes.
Because the bifurcation point depends on `μ` rather than on the difference, at
fixed `β` the swarm **deadlocks over two equally bad options and picks freely
between two equally good ones**, which is adaptive and which no classical
accumulator model reproduces. Weber's law falls out: the minimum discriminable
difference is `Δv_min = k·μ` with the Weber fraction proportional to `β`.

**It is a leaky competing accumulator.** Usher & McClelland, *Psychological
Review* 108(3):550–592 (2001), give `dxᵢ ∝ input − leak·xᵢ − β·Σ_{j≠i} x_j`.
Term for term the swarm model is an LCA with one extra constraint —
`x_u = 1 − Σxᵢ`, a conserved population — and that conservation is precisely
what produces value sensitivity.

**Direct switching is optimal; indirect switching is not.** Marshall, Bogacz,
Dornhaus, Planqué, Kovacs & Franks, *J. R. Soc. Interface* 6:1065–1074 (2009).
If a committed scout can be recruited *straight* to a rival site, the model
reduces (at zero decay) to the diffusion model and is exactly equivalent to
Wald's Sequential Probability Ratio Test, which is provably optimal. If she must
revert to uncommitted first, the model cannot be reduced to one dimension and is
not optimal.

> **What this workspace would have to represent.** `!object >N` is `β`, and the
> library has it. It has no `α_i(v_i)` — nothing lets evidence change the room's
> estimate of a topic's *value*, which is the term a refuting fact should move,
> and which [`0003`](../adr/0003-refutation-links-evidence-to-a-topic.md) adds.
> It has no conserved `x_u`: `TopicStanding` counts supporters without
> representing the uncommitted, so the finite-pool effect that makes bee
> decisions value-sensitive is absent. And support carries no intensity — a
> `!support` is worth `importance(Support)` whether the author is certain or
> indifferent, where a waggle dance carries the estimate in its length.

## Response thresholds and division of labour

Bonabeau, Theraulaz & Deneubourg, *Proc. R. Soc. B* 263:1565–1569 (1996) and
*Bulletin of Mathematical Biology* 60:753–807 (1998). Each individual carries a
per-task threshold `θᵢⱼ`; each task emits a stimulus `sⱼ` that rises while the
task goes undone. Engagement probability *(canonical form)*:

    T(sⱼ) = sⱼⁿ / (sⱼⁿ + θᵢⱼⁿ),     n = 2 in the original

with the loop closed by `dsⱼ/dt = δⱼ − α·N_active/N`. Two castes with
`θ₁ < θ₂` produce the whole allocation: the low-threshold caste engages first,
and only when demand outstrips them does `s` rise enough to conscript the
others. **Task allocation with no allocator, no queue, and no messages.**

Theraulaz, Bonabeau & Deneubourg, *Proc. R. Soc. B* 265:327–332 (1998), make
thresholds plastic: `θ` falls by `ξ` per unit time spent on the task and rises
by `φ` per unit time not spent, clamped to `[θ_min, θ_max]`. Doing a task makes
you likelier to do it again, so **specialization emerges from an initially
homogeneous population** — and reverses when the specialist leaves, because `φ`
brings everyone else back down.

> **What this workspace would have to represent.** `AgentThreshold` and
> `SPEAK_COST` implement the fixed-threshold half: speaking raises the speaker's
> threshold, silence lowers everyone else's. There is no `ξ`/`φ` on *topic*
> affinity — `AgentThreshold.affinity` is a static list the host supplies and
> the transcript never touches, so specialization can be configured but never
> learned. Deriving affinity from a member's own grounded deposits is a fold
> over traces the crate already holds.

## The limits

**Information cascades.** Bikhchandani, Hirshleifer & Welch, *Journal of
Political Economy* 100(5):992–1026 (1992); Banerjee, *QJE* 107(3):797–817 (1992).
A cascade occurs when it is optimal to follow predecessors **regardless of your
own private signal**. In the canonical binary model, once the first two agents
both adopt, the third adopts whatever her signal says — and from that moment her
action carries zero information, so the public belief freezes. Cascades start
almost immediately, can be wrong, and are never corrected by later arrivals.

The root cause is that agents observe **actions, not the evidence behind them**.
That sentence is the whole argument for `require_grounded`, and for
[`0004`](../adr/0004-grounds-are-weighed-by-evidential-depth.md).

**Condorcet and its failure.** Majority accuracy tends to 1 with group size if
each voter is independently right with `p > 1/2`. Both hypotheses carry the
weight: at `p < 1/2` the theorem runs backwards and large groups converge on
*certainly wrong*, and under common correlation `ρ` the effective number of
independent votes is bounded, so accuracy converges to a limit strictly below 1
(Ladha, *AJPS* 36:617–634, 1992).

**Diversity.** Page, *The Difference*, Princeton (2007). The diversity
prediction theorem is an identity, not a claim:

    (s̄ − θ)² = (1/n)Σ(sᵢ − θ)² − (1/n)Σ(sᵢ − s̄)²

collective error equals average individual error minus prediction diversity, so
collective error is always at most average individual error. That much is safe —
it is the bias–variance decomposition, and it is why bagging works. The stronger
"diversity trumps ability" claim is contested: Thompson, *Notices of the AMS*
61(9):1024–1030 (2014), and the replies from Kuehn (2015) and Singer (2019).
Cite the identity; do not cite the slogan.

**Group size.** Kao & Couzin, "Decision accuracy in complex environments is often
maximized by small group sizes", *Proc. R. Soc. B* 281:20133305 (2014). With one
low-correlation cue of reliability `r_L` attended with probability `p` and one
high-correlation cue shared across individuals, accuracy increases with group
size **only when `p > 1/(2·r_L)`**. When correlated cues dominate, accuracy is
maximized at a finite and often small group size. The mechanism is
counterintuitive and exact: a large group reliably reproduces the population-mean
opinion, which under a misleading shared cue is reliably wrong, while a small
group has enough sampling noise to sometimes land somewhere better.

This is the sharpest caution against reading the benchmark's
["across desk sizes"](../../wiki/Benchmarks.md) table as a recommendation to add
members. Those rooms are simulated with independent private evaluations, which
is `p = 1`. Real rooms share a brief.

**Hidden profiles.** Stasser & Titus, *JPSP* 48:1467–1478 (1985): groups discuss
what everyone already knows and fail to surface uniquely-held information, so a
group can systematically choose worse than its own members' pooled information
would support. This is the literature for what
[`checkout-503`](../../crates/tinyhivemind-hive/examples/bench/scenarios) is,
and it should be named on the wiki.

**The superorganism, honestly.** Boomsma & Gawne, *Biological Reviews* 93:28–54
(2018), reserve the term for lineages with irreversible lifetime commitment to a
reproductive division of labour, and note that a colony is not unitary — worker
policing, worker reproduction, and queen–worker conflict mean the optimized
entity "dissolves into a welter of conflicting cooperative and competitive
activities". For a room of agents with different prompts and different contexts:
do not assume aligned objectives merely because one team built the system.

## What recurs

Five of these are one equation under different names.

| | deposit | decay | cross-inhibition | threshold |
| --- | --- | --- | --- | --- |
| Ant trails | `Q/Lₖ`, quality-weighted | `(1−ρ)τ` | ACS local update `φ` | superlinear `n ≈ 2`, floor `k` |
| Honeybee | `α_i(v_i)·x_u` | `ρ_i·x_i` | `β·x_A·x_B` | pitchfork at `β* = 2ρ/μ` |
| Leaky competing accumulator | input | leak `k·xᵢ` | `β·Σ_{j≠i}x_j` | decision bound |
| Physarum (Tero et al., *Science* 327:439–442, 2010) | `Qᵘ/(1+Qᵘ)` | `r·D` | via flow conservation | the exponent `μ` |
| Bacterial quorum sensing | Hill autoinduction | `γA` | — | saddle-node, hysteretic |

So the minimal substrate is: a **conserved pool** of attention, a
**quality-weighted deposit**, a **decay**, a **bilinear inhibition**, and a
**nonlinear threshold**. `tinyhivemind-hive` has decay, inhibition, and a
threshold. It has no conserved pool, and its deposits are weighted by kind
rather than by quality — which is the same sentence, twice, as the two ADRs this
research produced.
