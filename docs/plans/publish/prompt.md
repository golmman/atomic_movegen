
To add a git tag retroactively:

Go to
https://crates.io/crates/atomic-movegen/0.1.0/code/.cargo_vcs_info.json

Add git tag via
```
git tag v0.1.0 eb378d3be22b512f2648f5f12cc1d91e18e643e9
```

Push via
```
git push --tags
```

---

Review if this lib is ready to be published. What's missing? Which best practices are violated? What should be done?

---

Nice analysis, thank you.

Here are some decisions and clarifications:
* exclude Fairy-Stockfish/ and docs/ in the package config
* only the package `atomic-movegen` is intended to be published, no examples
* CI config is not needed

Consider the decisions and clarifications and write an implementation plan to `docs/plans/publish/plan.md`.

---

I read the plan. Please clarify the following. Note that this is no request for change, i just need to understand.

My question: i need the different `[profile.*]` sections to distinguish between production builds and profiling builds, why should we remove this?

---

decisions:
* try to replace the unsafe

---

Review if this lib is ready to be published.
Note that we bumped the version to 2.0.0 after some breaking changes: `docs/plans/feedback/report.md`
