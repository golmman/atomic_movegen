Perform a thorough performance analysis.
Write your findings to `docs/plans/perf_analysis7/analysis.md`.

---

shakmaty is apparently less efficient than this project (`docs/shakmaty_comparison.md`).
Can we still learn something from its implementation?

Store your findings in `docs/plans/shakmaty/analysis.md'.

---

Last session we compiled this analysis: `docs/plans/shakmaty/analysis.md`.

I did not git commit yet since i see a major misconception:
There are NO mechanism to get extra commoners (kings) in atomic chess. Each side has at maximum 1 commoner. Promotions to commoners are not allowed.
Multi-commoner self-destruction is also not allowed.

Simple rule of thumb: Atomic chess boards have always 1 or more kings/commones but never more than 2. If a board has only 1 commoner it is a terminal position.

Please address this.

---

Last session we finished with
Please verify: perft-numbers did not change and performance did not regress.
