# Synthetic map metadata archives

These three v1 archives contain authored Galaxy text used as passive metadata. The StormLib v9.40 [fixture generator](../../../../adapters/starbank-mpq/tests/fixtures/generate.cpp) creates and independently reads each archive. Use the generator's `v1-no-list.s2ma` output. The repeated payload in `DocumentHeader` provides archive coverage only. The Rust header test constructs its own synthetic binary title record.

- `alpha.s2ma` declares the display author Blizzard Entertainment and asks for Shared and Second. SHA-256 `959c6ff7686d5884fa5e61f297286a254cbe66ad3a0d88e5bac3152fbec29d57`.
- `rival.s2ma` is a different map with its own display author that also claims Shared. SHA-256 `d744c12eefb203897215c8be129b55e98805dbaa23f43c528bec0d286c8b8306`.
- `empty.s2ma` claims a bank absent from the fixture inventory. SHA-256 `c5e17b4d7a7049e9bcbc9012e5240bc3e8efaf5d48f834ffebb70b75c79a8f17`.

No game files or player data were used.

## Metadata fallback fixtures

Both archives contain a synthetic bzip2-compressed primary Galaxy script. The MPQ adapter reports this compression as unsupported. The first archive supplies a readable alternate script and a title in `DocumentHeader`. The second supplies only the document title as a fallback.

- `fallback-script.s2ma` has an alternate author, title and declared bank. SHA-256 `2e69d6f3d73c6bcd278a77c624adefa838fbdd3eac7bd2460744e1a3f5a0398c`.
- `fallback-header.s2ma` has a binary title record and no alternate script. SHA-256 `69cf0c86670c76014425a6bbd7c65005bd6e77db8cb80051104f2171a2d72f9d`.

Build the pinned StormLib v9.40 library as in the [adapter fixture instructions](../../../../adapters/starbank-mpq/tests/fixtures/README.md). Set `$fixture_dir` to this fixture directory and run the following in a local temporary working directory.

```sh
c++ -std=c++17 -Wall -Wextra -Werror -IStormLib-9.40/src "$fixture_dir/generate-fallback.cpp" \
  storm-build/libstorm.a -o generate-fallback
python3 - <<'PY'
from pathlib import Path

primary = '// Name: Unavailable primary\n// Author: Unavailable author\nBankLoad("UnusableBank", 1);\n'
primary += '// Repeated synthetic text for bzip compression.\n' * 120
alternate = '// Name: Alternate Référence\n// Author: Fixture Reader\nBankLoad("FromAlternate", 1);\n'
Path("primary.galaxy").write_bytes(primary.encode("utf-8"))
Path("alternate.galaxy").write_bytes(alternate.encode("utf-8"))
Path("header.bin").write_bytes(b"DocInfo/Name.....\0Header fallback\x01\0")
PY
mkdir generated
./generate-fallback primary.galaxy alternate.galaxy header.bin generated
```
