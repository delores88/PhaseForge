# Method and protocol sources for the 0.4 discovery increment

Design references consulted September 2026; implementations and limitations are
specific to the delivered source, not claims that PhaseForge duplicates these systems.

1. Mouret & Clune, *Illuminating search spaces by mapping elites* (2015),
   https://arxiv.org/abs/1504.04909 . Motivation for a behavioral archive whose
   cells retain high-quality candidates. PhaseForge uses a bounded local variant.
2. SciPy official differential-evolution documentation,
   https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.differential_evolution.html .
   DE/rand/1/bin semantics informed the separate native optimizer. SciPy is not
   a new PhaseForge dependency.
3. Crossref official REST API documentation,
   https://www.crossref.org/documentation/retrieve-metadata/rest-api/ .
   Metadata discovery is not full-text retrieval or novelty assessment.
4. RO-Crate specification 1.1,
   https://www.researchobject.org/ro-crate/1.1/ .
   The export includes RO-Crate-style file/author JSON-LD metadata. No claim of
   passing a third-party RO-Crate profile/conformance validator is made here.
5. Google Research, *Accelerating scientific breakthroughs with an AI co-scientist*,
   https://research.google/blog/accelerating-scientific-breakthroughs-with-an-ai-co-scientist/ .
   Research discussion and iterative hypothesis refinement are separated from
   computational evidence. PhaseForge does not claim equivalent demonstrated
   discoveries or performance.

No copyrighted scientific engines or paid-license simulation packages were
bundled. Existing ecosystem discovery remains separate from actual execution adapters.
