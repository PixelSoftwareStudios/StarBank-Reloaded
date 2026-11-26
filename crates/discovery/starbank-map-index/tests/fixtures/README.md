# Synthetic map metadata archives

These three v1 archives contain authored Galaxy text used as passive metadata. The StormLib v9.40 [fixture generator](../../../../adapters/starbank-mpq/tests/fixtures/generate.cpp) creates and independently reads each archive. Use the generator's `v1-no-list.s2ma` output. The repeated payload in `DocumentHeader` provides archive coverage only. The Rust header test constructs its own synthetic binary title record.

- `alpha.s2ma` declares the display author Blizzard Entertainment and asks for Shared and Second. SHA-256 `959c6ff7686d5884fa5e61f297286a254cbe66ad3a0d88e5bac3152fbec29d57`.
- `rival.s2ma` is a different map with its own display author that also claims Shared. SHA-256 `d744c12eefb203897215c8be129b55e98805dbaa23f43c528bec0d286c8b8306`.
- `empty.s2ma` claims a bank absent from the fixture inventory. SHA-256 `c5e17b4d7a7049e9bcbc9012e5240bc3e8efaf5d48f834ffebb70b75c79a8f17`.

No game files or player data were used.
