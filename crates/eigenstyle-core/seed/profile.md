# The Porter voice profile

Built 2026-10-11 from Matt Porter's own published prose (corpus: see
`corpus-manifest.md`; measurements: `stylometry-raw.json`, produced by
`scripts/stylometry.py` — real counts, nothing estimated by eye). This
profile is meant to be read, questioned, and corrected by Matt; where the
numbers and the prose reading disagree, the numbers are the floor truth
and the reading is interpretation.

## The measured fingerprint (7,674 words, 6 pieces)

| Feature | Pooled value | What it means |
|---|---|---|
| Sentence length | mean 25.0, median 22, SD 17.5 words | Long average, very wide spread |
| Short sentences (≤10 words) | 13.4% | A real minority of punches |
| Long sentences (≥30 words) | 25.4% | A quarter of sentences are evidence-loaders |
| Paragraph length | mean 68 words | Substantial paragraphs, not fragments |
| Commas | 58.5 / 1,000 words | Dense subordination |
| Parentheticals | 14.1 / 1,000 words | Frequent asides, definitions, citations in-line |
| Hyphenated compounds | 15.4 / 1,000 words | "patient-centered", "post-quantum", "consumer-grade" |
| Semicolons | 1.4 / 1,000 words | Present, sparing |
| Colons | 4.2 / 1,000 words | Used to open explanations and lists |
| Em dashes | 0.4 / 1,000 words | Rare. He is not an em-dash writer |
| Questions | 3.1 / 1,000 words | Concentrated in FAQ and framing passages |
| Words ≥3 syllables | 27% | Technical register, unapologetically |
| Hedges | 3.7 / 1,000 words | Roughly balanced against assertives |
| Assertives (must/ensures/demonstrates/necessitates…) | 2.9 / 1,000 words | Claims land plainly, then get supported |
| First person | 9.6 / 1,000 words (we 4.0) | Genre-dependent: high in project prose (22.9), low in case reports (~3) |

Per-piece spread matters: the CORR letter runs 32.8% three-plus-syllable
words (his densest register); the project README is his most assertive
(7.2 assertives/1k) and most personal (22.9 first-person/1k); the case
reports nearly erase the first person and load parentheticals with data.

## The reading — how he writes

**Tone.** Plainspoken authority. He writes like a clinician-engineer
briefing peers: the technical term is used because it is the correct
term, then immediately made usable. In public prose he is wry about hype
and openly values-driven — safety, equity, and the vulnerable population
in the room are named, not implied.

**Argument shape.** News first, then the frame, then the evidence, then
the obligation. The CORR letter opens with the event (an executive order,
a summit), maps it onto his domain with an analogy, walks the technical
history, and closes by converting the analysis into what clinicians
must do. The README does the same in miniature: a TLDR line, then the
distinction that does the work (mathematical bias vs algorithmic/social
bias), then consequences. He argues by drawing a distinction and then
refusing to let the two halves be confused again.

**Cadence.** Bimodal. Long, comma-dense sentences carry the evidence —
a quarter of his sentences run 30+ words — and then a short sentence
lands the point. The short sentence is a verdict, not a transition.

**Signature constructions.**
- The "whether X or Y" parallel that yokes his two worlds into one claim
  (operating room and AI system in the same breath).
- The rhetorical question as a frame, answered immediately in prose.
- The TLDR colon-open: a compressed claim, then the unpacking.
- Definitions delivered parenthetically or by apposition, in-line, the
  first time a term appears.
- "we" for collective work and collective obligation; "I" is rare in
  the scholarly corpus.

**Stance.** Assert first, hedge precisely. Hedges cluster where the
evidence is genuinely incomplete (his discussion sections hedge at
~15/1k against a 2.6/1k corpus assertive baseline); framing and
conclusions are stated flatly. He does not hedge his values.

## Per genre

### Letter / commentary (sole-authored; the purest sample)
Opens on a dated, concrete event; compliments the target piece
genuinely ("their excellent column") before extending it; the analogy
to his own field does the argumentative work; closes with a single
sentence that converts analysis into duty.
Exemplar (CORR letter, closing): "Whether replacing arthritic knees or
healthcare AI systems, alignment necessitates deliberate
decision-making driven by clinically significant evidence with the goal
of delivering quality, patient-centered care."
And the frame, mid-letter: "Am I doing right, and in the right way?"

### Scholarly article (first-author papers and case reports)
Numbers-forward openings (incidence, projections, counts) before any
interpretation; parentheticals carry the data — scores, timepoints,
ranges; first person nearly disappears except a collective "we" in
methods; discussion sections hedge measurably more than introductions.
Exemplar (Joint Awareness Score paper): "Current projections of TKA
estimate 930,000 primary TKAs and approximately 202,966 knee revisions
for a total of 1,132,966 surgeries by 2030."

### Professional / project prose (README, public-facing)
The most personal register: "we" throughout, TLDR lines, direct
second-person answers in FAQ, and open impatience with hype — he mocks
"10X returns" and AGI-wrapper marketing by name. Values are stated as
design decisions (licensing as protection for vulnerable populations).
Exemplar: "TLDR - we do math to make AI ethically useful."
Exemplar: "These unfortunate folks identify themselves with their talk
of '10X' returns and building towards Artificial General Intelligence
aka 'AGI' while offering GPT wrappers."

### Peer review
NOT profiled. No verified Matt-authored review text was found in the
estate (the one review file present was agent-drafted in his format and
was quarantined — see the manifest). This section gets built when a
review he wrote himself enters the corpus.

## Negative checklist — markers that a draft is NOT his voice

- Em-dash stacking (his rate: 0.4/1k; generic AI prose runs 5–10× that).
- Uniform sentence length — no short verdict sentences, everything
  mid-length and smooth.
- Generic connective tissue: "furthermore", "moreover", "in
  conclusion", "it is important to note", "delve", "leverage",
  "landscape", "pivotal", "harnessing". His transitions are structural
  (a distinction, a question, a colon), not verbal filler.
- Hype vocabulary used straight — "10X", "revolutionary", "game-changer",
  AGI boosterism. He only quotes these to puncture them.
- Hedging the values: safety/equity claims stated tentatively. He
  hedges evidence, never the obligation.
- Burying the news: three paragraphs of background before the point.
  He leads with the event or the TLDR.
- First-person singular in scholarly work, or an absent "we" in project
  prose (the registers are near-mirror images; mixing them is a tell).
- Exclamation marks in scholarly prose (zero in the corpus outside
  one FAQ answer).

## Validation (2026-10-11) — and its honest limits

The checker (`scripts/check_voice.py`) scores stylometric distance
(Burrows-style mean |z| over 45 features) against bands calibrated from
the corpus pieces themselves (leave-one-out deltas 0.88–1.78; CLOSE ≤
1.78, DRIFTING ≤ 2.67).

| Test piece | Delta | Verdict |
|---|---|---|
| JAS paper, held-out 40% (genuine) | 1.196 | CLOSE |
| Cureus case report, held out whole (genuine) | 1.088 | CLOSE |
| Synthetic generic-AI passage, same topic (Pax-written, labeled) | 1.253 | CLOSE band, but FARTHER than both genuine pieces |

Read it straight: the checker ranks both genuine held-out pieces ahead
of the synthetic passage — the fingerprint measures something real —
but the margin is slim and the synthetic still lands inside the wide
CLOSE band. The causes are structural: the corpus is small and mixes
genres, so the reference spread (and therefore the bands) is wide; and
formal scholarly function-word profiles overlap across authors. Treat
CLOSE as necessary, not sufficient: the checker's top-contributing-
features list is the working tool (it correctly fingers the synthetic's
zero short sentences and em-dash rate), and Matt's ear remains the gate.
The profile strengthens with every sole-authored piece and genuine
review added to the corpus; bands should be recalibrated then
(`build_fingerprint.py` in the build workspace).
