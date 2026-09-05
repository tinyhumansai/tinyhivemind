# Delegation to experts and specialists

How a collective decides *which specialist acts*, in biology and in current
agentic systems, and which of those mechanisms can be encoded as a pure fold
over an attributed transcript with fixed-point integer arithmetic. This is the
reading behind [`../specs/expert-delegation.md`](../specs/expert-delegation.md)
and [ADR 0007](../adr/0007-the-directory-is-folded-from-citations.md).

## The gap

Before this work the library could express "who is here" and "who spoke", and
could not express "who knows". `RosterMember` is `{id, name}`; the only
expertise-shaped field, `AgentThreshold.affinity`, was host-supplied and never
written by the library; cross-desk referral routed to a *named* target's home
desk with nothing choosing the target by expertise; the responder ladder's
selector saw a role string and nothing derived from the transcript.

## Biology and collective behaviour

For each mechanism: the rule an individual runs on local information, then
what a transcript would have to represent to fold it.

**Fixed response thresholds** (Bonabeau, Theraulaz & Deneubourg 1996; 1998).
Individual *i* carries a private threshold θᵢⱼ per task *j*; task *j* emits a
public stimulus sⱼ that rises while undone; engage with probability
sⱼⁿ/(sⱼⁿ + θᵢⱼⁿ), n = 2. Two castes with θ₁ < θ₂ produce the whole allocation
with no allocator and no messages. *Fold:* the attention market's argmax over
`urge - threshold` already is this. The missing half is that stimulus is not
per task: salience is per trace and relevance is a static lookup.

**Threshold reinforcement** (Theraulaz, Bonabeau & Deneubourg 1998). θ falls by
ξ per unit time on the task and rises by φ off it, clamped to [θ_min, θ_max].
Specialists emerge from a homogeneous population and the specialisation
reverses when the specialist leaves. The clamp is load-bearing; MAX–MIN Ant
System (Stützle & Hoos 2000) uses the same clamp against premature
convergence. *Fold:* affinity as a count of own deposits on a topic, clamped,
which is commutative; iterated updates are not.

**Foraging for work** (Tofts & Franks 1992; Franks & Tofts 1994). No thresholds:
wander, do work where found, drift to an adjacent zone otherwise. *Fold:*
topic adjacency is co-citation, a graph over `cites`. Probably over-engineering.

**Interaction-rate encoding** (Gordon 1996; Gordon & Mehdiabadi 1999; Greene &
Gordon 2007; Prabhakar, Dektar & Gordon 2012). A harvester ant decides whether
to forage from the *rate* of brief contacts with returning foragers, not from
what any contact says. *Fold:* the arrival rate of a topic's traces over a
window is itself an allocation signal, and the library does not measure it.

**Honeybee recruitment.** The waggle dance advertises a site with intensity
proportional to the scout's own absolute private estimate (Seeley & Visscher
2008); roughly 5% of bees ever visit more than one site, so nobody compares.
Advertising decays by a fixed absolute amount per return (Seeley & Visscher
2003), a quality-proportional survival filter. The **tremble dance** (Seeley
1992) fires when a forager waits more than about 50 s to unload: it recruits a
*different caste* and suppresses recruitment of more foragers. A load signal
that reallocates labour across castes, triggered by a locally measured
queueing delay. The **stop signal** (Seeley et al. 2012) is cross-inhibition
targeted at dancers for other sites, already implemented here as `!object`.
The **tandem-run to carrying switch** at quorum (Pratt et al. 2002; Pratt 2005)
moves the colony from a mode that preserves independent judgement to one that
does not, and maps onto `Deliberate → Commit`. *Fold:* an intensity must be
earned from `cites`, never parsed from prose, or it is free to inflate.

**Teaching and informed leaders.** Tandem running is teaching by the strict
criteria, at about four times the solo journey time (Franks & Richardson
2006): an expert who transfers knowledge is slower than one who acts. Couzin,
Krause, Franks & Levin (2005): a small informed minority steers a group whose
naive members cannot tell who is informed, and the proportion needed shrinks
with group size. *Fold:* a directory does not need to be broadcast to work; it
only needs to bias the argmax.

**Transactive memory.** Wegner (1986; 1995): a group's memory is the
*directory*, not the contents, with directory updating, information allocation
and retrieval coordination. Lewis (2003) validates specialisation,
**credibility** and coordination; credibility is what lets a retriever trust
an owner instead of re-deriving. Hollingshead (1998; 2000): teams rely on
diffuse cues (role label) early and specific cues (observed experience) as
they gain shared history; the effect of diffuse cues falls with experience.
*A host-supplied role string is a diffuse cue; a folded record of grounded
deposits is a specific one.*

**Hidden profiles.** Stasser & Titus (1985): groups discuss what everyone
knows and fail to surface uniquely held items. Stasser & Stewart (1992): the
failure largely disappears under a "demonstrably correct answer" framing.
Stewart & Stasser (1995) and Stasser, Stewart & Wittenbaum (1995): public
**expert role assignment** raises unique-item sampling, modestly (about 29% to
34%; Lu, Yuan & McLeod 2012 meta-analysis). *Rule:* announce my domain; when a
question falls in another's announced domain, ask them rather than answer;
when it falls in mine, contribute even unasked.

**Switching cost.** Leighton, Charbonneau & Dornhaus (2017): the interval
between acts is shorter when an individual repeats the same task, in every
active worker group and independently of whether the worker is a specialist.
Division of labour buys the avoidance of switching delay, not better workers.
*Fold:* a fixed penalty when the topic a member would speak on differs from
its last one; `SPEAK_COST`'s mirror image.

**Quorum and pooling.** Sumpter & Pratt (2009): quorum responses need k ≥ 2 and
actively suppress early social influence below the threshold. Marshall et al.
(2019): simple majority is frequently sub-optimal.

## Agentic systems, 2023 to 2026

| system | who acts next | signal | fan-out | failure mode |
| --- | --- | --- | --- | --- |
| AutoGen 0.2 GroupChat | `auto` (LLM names next speaker), round-robin, manual, callable | LLM over full history | sequential | transcript read per turn |
| AutoGen FSM / 0.4 SelectorGroupChat | allowed-transitions graph ∩ LLM; `selector_func`, `candidate_func` | static graph + LLM | sequential | wrong edges deadlock silently |
| LangGraph supervisor, hierarchical teams | supervisor node emits `Command(goto)` | LLM, static topology | sequential | supervisor is a context bottleneck |
| CrewAI hierarchical | manager calls "delegate to coworker" by `role` string | LLM matching task to role | sequential | delegation loops |
| OpenAI Swarm / Agents SDK | handoff as a tool; control fully transferred | LLM tool choice | sequential | no return edge |
| MetaGPT | publish/subscribe on `cause_by` | static subscriptions | broadcast | bounded only by disjoint watch sets |
| CAMEL, ChatDev | fixed dyad; waterfall SOP | topology | sequential | role flipping, loops |
| Anthropic research system (2025) | orchestrator spawns 3–5 workers | LLM decomposition | parallel | ~15× tokens; usage explains ~80% of variance |
| Claude Code subagents and skills | task matched against `description` | LLM over a short index | either | the description is the router; misroutes are silent |

Learned and scored selection: **DyLAN** (Liu et al. 2023) computes an Agent
Importance Score by backward aggregation of peer ratings and keeps the top-k,
the closest published analogue to a citation-folded directory. **AgentVerse**
recruits generated expert personas (unfalsifiable). **GPTSwarm** (Zhuge et al.
2024) learns who talks to whom. **MasRouter** (2025) jointly picks
collaboration mode, roles and per-agent model, with up to 52% overhead
reduction. **AgentRouter** (2025) routes over a knowledge graph. **Bala & Shah
(2026)** frame routing as set-valued prediction and find supervised routers
beat zero-shot LLM routing: "ask a model who should answer" is the baseline,
not the ceiling. Mixture-of-Agents (Wang et al. 2024) improves even from worse
auxiliary answers and is rarely reported against a matched-budget control.

Cost-aware cascades: **FrugalGPT** (cheapest first, escalate on low scorer
confidence, a quorum on a confidence signal), **RouteLLM**, **Hybrid LLM**,
**RouterBench**, **IRT-Router**. Predictive routing decides from the query;
cascade routing needs an abstention signal the answerer emits. **KnowNo**
(Ren et al. 2023) is the principled abstention trigger: ask for help when the
conformal prediction set is not a singleton. Expert personas
(**ExpertPrompting**, multi-expert prompting) are diffuse cues and can trade
accuracy for alignment.

### The honest half

1. **The compute confound.** Token usage explains most multi-agent gains;
   matched-budget self-consistency is the control and this repository's `vote`
   arm stays as it is.
2. **MAST** (Cemri et al. 2025): 14 failure modes from 1600+ traces across
   seven frameworks, 41% to 87% failure rates. Step repetition (15.7%),
   unawareness of termination (12.4%) and disobeying the task (11.8%) dominate;
   **role violation is 1.5%**. An expertise layer buys little against that
   distribution and adds surface for the first three.
3. **Convergence.** Conformity rises with a peer's apparent capability, and a
   directory amplifies apparent capability by construction.
4. **Directory circularity.** DyLAN's score, Claude Code's description match
   and any transcript-folded affinity share a defect: who spoke becomes who is
   thought to know. That is an information cascade with a routing table.
5. **Every added verb here has lost.** `!refute` cost 7 points and dropped
   below the vote control; `require_evidential` cost 26. A new verb is a new
   way to spend turns on meta-conversation.

## Synthesis: six mechanisms, ranked

1. **A folded transactive-memory directory feeding `BidReason::Knows`.**
   Wegner + Lewis + DyLAN. Specialisation = decayed grounded deposits on a
   topic; credibility = decayed citations by others minus objections; host
   affinity as a prior. No new verb. Benchmark: a hidden profile with
   heterogeneous expertise; the acceptance criterion is already written in the
   shared-medium spec. Might lose because: uniform-expertise benches give it
   nothing to route on (expected gain there is zero); circularity; mutual
   citation is gameable; a 6–7 turn episode is short for any estimator.
2. **Render the directory into the room.** Hutchins's speed bugs; Stasser's
   expert-role assignment. Host pastes `Directory::lines()` into the prompt.
   Small honest effect; also renders the prior.
3. **`!defer` as abstention and cost-aware escalation.** Tremble dance;
   FrugalGPT. Not a wasted turn but one bit of high-value directory evidence.
   Might lose because a turn costs 100% of a turn where a cheap model call costs
   about 1% of an expensive one; miscalibrated deferral is the dominant failure
   of escalation systems; defer chains are a step-repetition surface.
4. **Threshold reinforcement plus a switching cost.** No wire change. Hard to
   justify at 6.75 turns per episode without cross-episode persistence the
   crate deliberately does not own.
5. **Expertise-directed referral.** Couzin; LangGraph supervisor. The directory
   is folded from *this* conversation, so another desk's members weigh zero
   unless a host supplies cross-desk priors, and a returned answer is an
   unrebuttable cascade seed.
6. **`!offer` with an earned quality signal.** Waggle dance. Pure meta; the
   repository's record on added verbs argues against it.

### Acceptance criterion, written before the numbers

The mechanism must be able to lose and the loss must be published; the bench
arm and scenario family land before the mechanism and are fixed before tuning;
`vote` gets the same turn budget and results are reported at equal turns and as
turns-to-decision at equal accuracy; a mechanism that helps hidden profiles but
costs more than two points on the uniform 5000-room bench ships off by default;
directory circularity is reported as the rank correlation between directory
weight and speech share; the predicted result on the uniform bench is zero.

## References

- Bonabeau, Theraulaz & Deneubourg, *Proc. R. Soc. B* 263:1565 (1996); *Bull. Math. Biol.* 60:753 (1998).
- Theraulaz, Bonabeau & Deneubourg, *Proc. R. Soc. B* 265:327 (1998).
- Robinson, *Annu. Rev. Entomol.* 37:637 (1992). Tofts & Franks, *TREE* 7:346 (1992); Franks & Tofts, *Anim. Behav.* 48:470 (1994). Tripet & Nonacs, *Ethology* 110:863 (2004).
- Gordon, *Nature* 380:121 (1996); Gordon & Mehdiabadi, *Behav. Ecol. Sociobiol.* 45:370 (1999); Greene & Gordon, *Behav. Ecol.* 18:451 (2007); Prabhakar, Dektar & Gordon, *PLoS Comput. Biol.* (2012).
- Seeley, *Behav. Ecol. Sociobiol.* 31:375 (1992); Seeley & Visscher, *Behav. Ecol. Sociobiol.* 54:511 (2003); *J. Exp. Biol.* 211:3691 (2008); Seeley et al., *Science* 335:108 (2012).
- Pratt, Mallon, Sumpter & Franks, *Behav. Ecol. Sociobiol.* 52:117 (2002); Pratt, *Behav. Ecol.* 16:488 (2005).
- Franks & Richardson, *Nature* 439:153 (2006). Couzin, Krause, Franks & Levin, *Nature* 433:513 (2005).
- Leighton, Charbonneau & Dornhaus, *Behav. Ecol.* 28:319 (2017).
- Sumpter & Pratt, *Phil. Trans. R. Soc. B* 364:743 (2009); Marshall et al., *eLife* 8:e40368 (2019); Kao & Couzin, *Proc. R. Soc. B* 281:20133305 (2014).
- Czaczkes, Grüter & Ratnieks, *Annu. Rev. Entomol.* 60:581 (2015). Stützle & Hoos, *FGCS* 16:889 (2000).
- Wegner, in Mullen & Goethals, *Theories of Group Behavior* (1986); *Social Cognition* 13:319 (1995). Lewis, *J. Appl. Psychol.* 88:587 (2003). Hollingshead, *Group Processes & Intergroup Relations* 3:257 (2000).
- Stasser & Titus, *JPSP* 48:1467 (1985); Stasser & Stewart, *JPSP* 63:426 (1992); Stewart & Stasser, *JPSP* 69:619 (1995); Stasser, Stewart & Wittenbaum, *JESP* 31:244 (1995); Lu, Yuan & McLeod, *PSPR* (2012).
- Hutchins, *Cognition in the Wild* (1995).
- AutoGen speaker selection: microsoft.github.io/autogen (0.2 docs, FSM GroupChat blog, 2024). LangGraph supervisor: github.com/langchain-ai/langgraph-supervisor-py. CrewAI hierarchical process: docs.crewai.com. OpenAI Swarm: github.com/openai/swarm. MetaGPT communication docs. CAMEL arXiv:2303.17760; ChatDev arXiv:2307.07924.
- Anthropic, "How we built our multi-agent research system" (2025). Claude Code subagents documentation.
- Wang et al., Mixture-of-Agents, arXiv:2406.04692. Liu et al., DyLAN, arXiv:2310.02170. Chen et al., AgentVerse, arXiv:2308.10848. Zhuge et al., GPTSwarm, ICML 2024. MasRouter arXiv:2502.11133. AgentRouter arXiv:2510.05445. Bala & Shah arXiv:2606.28925 (2026). Ishibashi & Nishimura, Self-Organized Agents, arXiv:2404.02183.
- Chen, Zaharia & Zou, FrugalGPT, arXiv:2305.05176. Ong et al., RouteLLM, arXiv:2406.18665. Ding et al., Hybrid LLM, arXiv:2404.14618. Hu et al., RouterBench, arXiv:2403.12031. RouterEval arXiv:2503.10657.
- Xu et al., ExpertPrompting, arXiv:2305.14688. Multi-expert prompting, arXiv:2411.00492. Ren et al., KnowNo, arXiv:2307.01928.
- Cemri et al., "Why Do Multi-Agent LLM Systems Fail?", arXiv:2503.13657. "Multi-LLM-Agents Debate: Performance, Efficiency, and Scaling Challenges", ICLR 2025 blogpost track.

Items with 2026 arXiv identifiers surfaced through search snippets; only
arXiv:2606.28925 was fetched directly. Verify the rest against the primary PDF
before citing them elsewhere.
