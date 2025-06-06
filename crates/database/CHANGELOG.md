# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0](https://github.com/bluealloy/revm/compare/revm-database-v1.0.0-alpha.5...revm-database-v1.0.0) - 2025-03-24

### Other

- docs nits ([#2292](https://github.com/bluealloy/revm/pull/2292))

## [1.0.0-alpha.5](https://github.com/bluealloy/revm/compare/revm-database-v1.0.0-alpha.4...revm-database-v1.0.0-alpha.5) - 2025-03-21

### Other

- make clippy happy ([#2274](https://github.com/bluealloy/revm/pull/2274))
- simplify single UT for OpSpecId compatibility. ([#2216](https://github.com/bluealloy/revm/pull/2216))

## [1.0.0-alpha.4](https://github.com/bluealloy/revm/compare/revm-database-v1.0.0-alpha.3...revm-database-v1.0.0-alpha.4) - 2025-03-16

### Other

- updated the following local packages: revm-primitives, revm-bytecode

## [1.0.0-alpha.3](https://github.com/bluealloy/revm/compare/revm-database-v1.0.0-alpha.2...revm-database-v1.0.0-alpha.3) - 2025-03-11

### Fixed

- correct propagate features ([#2177](https://github.com/bluealloy/revm/pull/2177))

### Other

- bump alloy ([#2183](https://github.com/bluealloy/revm/pull/2183))

## [1.0.0-alpha.2](https://github.com/bluealloy/revm/compare/revm-database-v1.0.0-alpha.1...revm-database-v1.0.0-alpha.2) - 2025-03-10

### Fixed

- use correct HashMap import ([#2148](https://github.com/bluealloy/revm/pull/2148))
- *(op)* Handler deposit tx halt, catch_error handle ([#2144](https://github.com/bluealloy/revm/pull/2144))

### Other

- *(db)* separate fields from `CacheDB` into `Cache` ([#2131](https://github.com/bluealloy/revm/pull/2131))
- PrecompileErrors to PrecompileError ([#2103](https://github.com/bluealloy/revm/pull/2103))
- *(deps)* bump breaking deps ([#2093](https://github.com/bluealloy/revm/pull/2093))
- move all dependencies to workspace ([#2092](https://github.com/bluealloy/revm/pull/2092))
- re-export all crates from `revm` ([#2088](https://github.com/bluealloy/revm/pull/2088))

## [1.0.0-alpha.1](https://github.com/bluealloy/revm/releases/tag/revm-database-v1.0.0-alpha.1) - 2025-02-16

### Added

- Context execution (#2013)
- EthHandler trait (#2001)
- expose precompile address in Journal, DB::Error: StdError (#1956)
- integrate codspeed (#1935)
- *(database)* implement order-independent equality for Reverts (#1827)
- couple convenience functions for nested cache dbs (#1852)
- Restucturing Part7 Handler and Context rework (#1865)
- add support for async database (#1809)
- restructure Part2 database crate (#1784)
- *(examples)* generate block traces (#895)
- implement EIP-4844 (#668)
- *(Shanghai)* All EIPs: push0, warm coinbase, limit/measure initcode (#376)
- Migrate `primitive_types::U256` to `ruint::Uint<256, 4>` (#239)
- Introduce ByteCode format, Update Readme (#156)

### Fixed

- no_std for revm-database (#2077)
- fix typos ([#620](https://github.com/bluealloy/revm/pull/620))

### Other

- set alpha.1 version
- Bump licence year to 2025 (#2058)
- add comment for pub function and fix typo (#2015)
- bump alloy versions to match latest (#2007)
- fix comments and docs into more sensible (#1920)
- bumps select alloy crates to 0.6 (#1854)
- *(TransitionAccount)* remove unneeded clone (#1860)
- *(CacheAccount)* remove unneeded clone (#1859)
- bump alloy to 0.4.2 (#1817)
- *(primitives)* replace HashMap re-exports with alloy_primitives::map (#1805)
- Bump new logo (#1735)
- *(README)* add rbuilder to used-by (#1585)
- added simular to used-by (#1521)
- add Trin to used by list (#1393)
- Fix typo in readme ([#1185](https://github.com/bluealloy/revm/pull/1185))
- Add Hardhat to the "Used by" list ([#1164](https://github.com/bluealloy/revm/pull/1164))
- Add VERBS to used by list ([#1141](https://github.com/bluealloy/revm/pull/1141))
- license date and revm docs (#1080)
- *(docs)* Update the benchmark docs to point to revm package (#906)
- *(docs)* Update top-level benchmark docs (#894)
- clang requirement (#784)
- Readme Updates (#756)
- Logo (#743)
- book workflow ([#537](https://github.com/bluealloy/revm/pull/537))
- add example to revm crate ([#468](https://github.com/bluealloy/revm/pull/468))
- Update README.md ([#424](https://github.com/bluealloy/revm/pull/424))
- add no_std to primitives ([#366](https://github.com/bluealloy/revm/pull/366))
- revm-precompiles to revm-precompile
- Bump v20, changelog ([#350](https://github.com/bluealloy/revm/pull/350))
- typos (#232)
- Add support for old forks. ([#191](https://github.com/bluealloy/revm/pull/191))
- revm bump 1.8. update libs. snailtracer rename ([#159](https://github.com/bluealloy/revm/pull/159))
- typo fixes
- fix readme typo
- Big Refactor. Machine to Interpreter. refactor instructions. call/create struct ([#52](https://github.com/bluealloy/revm/pull/52))
- readme. debuger update
- Bump revm v0.3.0. README updated
- readme
- Add time elapsed for tests
- readme updated
- Include Basefee into cost calc. readme change
- Initialize precompile accounts
- Status update. Taking a break
- Merkle calc. Tweaks and debugging for eip158
- Replace aurora bn lib with parity's. All Bn128Add/Mul/Pair tests passes
- TEMP
- one tab removed
- readme
- README Example simplified
- Gas calculation for Call/Create. Example Added
- readme usage
- README changes
- Static gas cost added
- Subroutine changelogs and reverts
- Readme postulates
- Spelling
- Restructure project
- First iteration. Machine is looking okay
