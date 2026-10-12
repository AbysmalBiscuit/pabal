# Changelog

## [0.3.0](https://github.com/AbysmalBiscuit/pabal/compare/v0.2.2...v0.3.0) (2026-10-12)


### ⚠ BREAKING CHANGES

* `AnySessionStart::add_context` now returns `Option<Response>` (`None` for Grok) instead of `AnySessionStart` implementing `AddContext`. The exhaustive public enums `AnyHarness`, `AnyPayload` and the per-event `Any*` views (`AnySessionStart`, `AnyPreToolUse`, `AnyStop`, ...) gain `Grok` variants, so exhaustive matches on them stop compiling.

### Features

* add the Grok harness ([#21](https://github.com/AbysmalBiscuit/pabal/issues/21)) ([c2d4874](https://github.com/AbysmalBiscuit/pabal/commit/c2d4874d626ef227a1a25a28283d131fcc02df61))

## [0.2.2](https://github.com/AbysmalBiscuit/pabal/compare/v0.2.1...v0.2.2) (2026-10-04)


### Features

* let a Stop hook block the stop ([#18](https://github.com/AbysmalBiscuit/pabal/issues/18)) ([4b7317e](https://github.com/AbysmalBiscuit/pabal/commit/4b7317e143f1e22d5cb0c6162964bb45c5d217b0))

## [0.2.1](https://github.com/AbysmalBiscuit/pabal/compare/v0.2.0...v0.2.1) (2026-10-03)


### Features

* add a PreToolUse input rewrite answer ([#15](https://github.com/AbysmalBiscuit/pabal/issues/15)) ([7543f86](https://github.com/AbysmalBiscuit/pabal/commit/7543f861cc3335c3c300e171ba0c2f54b0491cac))

## [0.2.0](https://github.com/AbysmalBiscuit/pabal/compare/v0.1.0...v0.2.0) (2026-09-27)


### ⚠ BREAKING CHANGES

* add Cursor and Antigravity harnesses ([#9](https://github.com/AbysmalBiscuit/pabal/issues/9))

### Features

* add Cursor and Antigravity harnesses ([#9](https://github.com/AbysmalBiscuit/pabal/issues/9)) ([122dddd](https://github.com/AbysmalBiscuit/pabal/commit/122dddd340c6b7ed2843ccb307e20efd58e1a0d6))

## 0.1.0 (2026-09-27)


### Features

* add pabal v0.1.0 ([#1](https://github.com/AbysmalBiscuit/pabal/issues/1)) ([23566a0](https://github.com/AbysmalBiscuit/pabal/commit/23566a0f47fbe380b5f163a13a6933785790919b))
