DONE: package name with "-" ? -> rust convention
TODO: deployment to crates.io
DONE: code audit, review, cleanup
TODO: readme: update, document examples
TODO: rust docs
TODO: ideas from https://github.com/ankan-ban/perft_gpu

---

Perform a code audit.
Focus on performance, inconsistencies, DRY, YAGNI, KISS, unnecesary/outdated comments.
Create a plan for the cleanup in `docs/plans/cleanup/plan-code-cleanup2.md`

Add a report to `docs/plans/cleanup/report-code-cleanup.md`. 

---

Please investigate:
* why is the attacks module public?
  * also: the functions in this module are not documented consistently
* why is the bitboard module public?
* why is the magic module public?
* why is the pext module public?
* why do we need the perft function? It is already an example
* shouldn't we document the examples?

Don't implement anything yet, we just want to know the things which could need a little housekeeping.
