# Changelog

Every version of golem released off `main`, rendered by
[git-cliff](https://git-cliff.org) from the conventional commits behind each
tag. `release` rewrites this file in full on every release, so an edit made
here is lost — edit the commit messages instead.

v0.1.0 and v0.2.0 have no section of their own. Both tags name a commit that was
never on `main`, so there is no range for git-cliff to walk between them and the
work they were meant to mark is folded into v0.3.0.

## v0.4.0 — 2026-08-09

### Features

- release from commits ([#21](https://github.com/dull-ca/golem/pull/21)) ([14b18cb](https://github.com/dull-ca/golem/commit/14b18cbc1e55d1dca9ea093e1e50cab9a0c0d0ec))

### Tooling

- updated docs and readme ([#20](https://github.com/dull-ca/golem/pull/20)) ([a4b78c7](https://github.com/dull-ca/golem/commit/a4b78c7caf2fa6517d32226c7c6d36f04b2830f0))

## v0.3.1 — 2026-08-09

### Features

- release tooling ([#18](https://github.com/dull-ca/golem/pull/18)) ([5c1afef](https://github.com/dull-ca/golem/commit/5c1afef2d508ad030745919c1d89e2e24eeeeb29))
- **website**: give the docs site the favicon its head already links ([#19](https://github.com/dull-ca/golem/pull/19)) ([abcc88b](https://github.com/dull-ca/golem/commit/abcc88bfcbc245c0f26a41a7414b839e704b3ee3))

## v0.3.0 — 2026-08-09

### Features

- add devenv ([65fc682](https://github.com/dull-ca/golem/commit/65fc68236d333e848d8a3056d1abae590c642549))
- version -0.1. Fully just LLM fueled fever dream at this point ([b6a610f](https://github.com/dull-ca/golem/commit/b6a610fdae255b96f0c0efd5fb82508c0a99faee))
- very rough initial docs ([f2d15b3](https://github.com/dull-ca/golem/commit/f2d15b33678681b5440532db2620777c1d40faf1))
- CI with cachix cache ([#1](https://github.com/dull-ca/golem/pull/1)) ([bbfe5ed](https://github.com/dull-ca/golem/commit/bbfe5ed77312e0c8324563b57052195dbefa0e08))
- plan mode + better lsp features.  ([#2](https://github.com/dull-ca/golem/pull/2)) ([d573c2b](https://github.com/dull-ca/golem/commit/d573c2b3654fc8ffc1b9549f26e7097f5517fdd6))
- fleet fanout nftables ([#3](https://github.com/dull-ca/golem/pull/3)) ([984998c](https://github.com/dull-ca/golem/commit/984998cfc9c617c7b3d5a6ad6a2c2fcc3b90cc17))
- ssh &  auth ([#4](https://github.com/dull-ca/golem/pull/4)) ([0f521e0](https://github.com/dull-ca/golem/commit/0f521e0505783f649ff782010da238af3d405c2a))
- **quadlet**: labels and networks, and a Traefik-fronted LimeSurvey example ([#5](https://github.com/dull-ca/golem/pull/5)) ([6a61b86](https://github.com/dull-ca/golem/commit/6a61b866f1cc3c1077223ee99ded6c05fff5b7b3))
- record updates ([#6](https://github.com/dull-ca/golem/pull/6)) ([2093611](https://github.com/dull-ca/golem/commit/20936111c46bae4d431e2d8472a28bbaf7a742f7))
- secretspec integration ([#9](https://github.com/dull-ca/golem/pull/9)) ([6fffec5](https://github.com/dull-ca/golem/commit/6fffec5708da7dc51db68a21de7803ba27e0e995))
- full module system ([#10](https://github.com/dull-ca/golem/pull/10)) ([a3b3eb0](https://github.com/dull-ca/golem/commit/a3b3eb0cf7dfa2b41b926bc217b1f47dd9a672a8))
- **lib**: the three things dulliac needs from Traefik and Quadlet ([#11](https://github.com/dull-ca/golem/pull/11)) ([1e93bd5](https://github.com/dull-ca/golem/commit/1e93bd54eeaee8c4adb4a171d87873c1a1eeb754))
- **emet**: a constructor is identified by its module ([#13](https://github.com/dull-ca/golem/pull/13)) ([077be41](https://github.com/dull-ca/golem/commit/077be41d7eb9b0bdcc3e5938f1f3117396e88390))
- **emet**: restrict `exposing` to locally declared names ([#15](https://github.com/dull-ca/golem/pull/15)) ([2512ea4](https://github.com/dull-ca/golem/commit/2512ea4db3c049370a845c784616ce44234a99db))
- build everything ([#17](https://github.com/dull-ca/golem/pull/17)) ([8bd3384](https://github.com/dull-ca/golem/commit/8bd3384895181cef1d40b1722cf185228597473a))
- **docs-image**: serve the docs with a TLS-less static nginx ([11b108d](https://github.com/dull-ca/golem/commit/11b108d52c4aa9dc4090673cbd4d390886798a9f))

### Fixes

- closer to what I'm envisioning ([e1f1c28](https://github.com/dull-ca/golem/commit/e1f1c28a2c68105dbd003ac7bca6c6c723c9d8be))
- updated docs ([6367741](https://github.com/dull-ca/golem/commit/636774148f14df44ce95174446b1e8a25be11996))
- updated docs to be closer to what I want. still not quite there. ([45c6efe](https://github.com/dull-ca/golem/commit/45c6efe5530a56fb099cb29ecdc6e2db7697e357))
- further updates ([3155583](https://github.com/dull-ca/golem/commit/3155583b46c2e5b86a6d79069f87d729004ddd4c))
- simplify docs ([f3ec7f0](https://github.com/dull-ca/golem/commit/f3ec7f0232659ddad967f8d3c6b0ae87a40a2305))
- config ([3b19ce2](https://github.com/dull-ca/golem/commit/3b19ce27d71776020552315048eb7a6598480154))
- slowly taking shape ([83b90f1](https://github.com/dull-ca/golem/commit/83b90f17f47af641a30545ac573f9b9b20485ff3))
- iterating towards something very simple. ([45eac67](https://github.com/dull-ca/golem/commit/45eac6781447ac45099cd6363326ce93e4283487))
- working towards a functional first version ([1514847](https://github.com/dull-ca/golem/commit/151484750e7f1b5b214a9795ec0344ea76714fef))
- import collision ([#7](https://github.com/dull-ca/golem/pull/7)) ([326075f](https://github.com/dull-ca/golem/commit/326075fc615347d528b7ce84564ea133ae98d341))
- ctor collision ([#8](https://github.com/dull-ca/golem/pull/8)) ([d55845b](https://github.com/dull-ca/golem/commit/d55845bc7196056c9767df2f31b044ec38ab0a52))

### Refactoring

- **scroll-format**: one fleet-key format, shared by both ends ([#14](https://github.com/dull-ca/golem/pull/14)) ([819f2ae](https://github.com/dull-ca/golem/commit/819f2ae27e0444040df6305cb45cf2b55919fbc6))

### Tooling

- updating docs ([7464cd8](https://github.com/dull-ca/golem/commit/7464cd87ae23689a016243c2a396f77f809da171))
- An initial attempt at simplifying this. ([#1](https://github.com/dull-ca/golem/pull/1)) ([0b7ee2d](https://github.com/dull-ca/golem/commit/0b7ee2d330427f61071a3563380710209e048d00))
- build and smoke-test the docs image, publish it on a tag ([#12](https://github.com/dull-ca/golem/pull/12)) ([5b751ce](https://github.com/dull-ca/golem/commit/5b751ce229219c4b1099a3e0fe51465743f240d8))

### Other

- Build and switch to a new language Emet, designed after elm, but for these purposes and update all docs/implementations. ([#2](https://github.com/dull-ca/golem/pull/2)) ([1d00e89](https://github.com/dull-ca/golem/commit/1d00e8984e82e4c5843bf9702bc7984fe148a2c6))

