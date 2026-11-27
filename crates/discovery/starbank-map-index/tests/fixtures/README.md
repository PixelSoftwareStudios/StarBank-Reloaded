# Synthetic map metadata archives

These three v1 archives contain authored Galaxy text used as passive metadata. The StormLib v9.40 [fixture generator](../../../../adapters/starbank-mpq/tests/fixtures/generate.cpp) creates and independently reads each archive. Use the generator's `v1-no-list.s2ma` output. The repeated payload in `DocumentHeader` provides archive coverage only. The Rust header test constructs its own synthetic binary title record.

- `alpha.s2ma` declares the display author Blizzard Entertainment and asks for Shared and Second. SHA-256 `959c6ff7686d5884fa5e61f297286a254cbe66ad3a0d88e5bac3152fbec29d57`.
- `rival.s2ma` is a different map with its own display author that also claims Shared. SHA-256 `d744c12eefb203897215c8be129b55e98805dbaa23f43c528bec0d286c8b8306`.
- `empty.s2ma` claims a bank absent from the fixture inventory. SHA-256 `c5e17b4d7a7049e9bcbc9012e5240bc3e8efaf5d48f834ffebb70b75c79a8f17`.

No game files or player data were used.

## Metadata fallback fixtures

The archives exercise two independent errors before trying the alternate Galaxy script or `DocumentHeader`. The adapter rejects unsupported bzip2 compression, and the metadata parser reports invalid text or excess bank references. Recovered fields remain visible with the original warning.

- `fallback-script.s2ma` has an alternate author, title and declared bank. SHA-256 `2e69d6f3d73c6bcd278a77c624adefa838fbdd3eac7bd2460744e1a3f5a0398c`.
- `fallback-header.s2ma` has a binary title record and no alternate script. SHA-256 `69cf0c86670c76014425a6bbd7c65005bd6e77db8cb80051104f2171a2d72f9d`.
- `invalid-script-with-alternate.s2ma` has a stored primary script containing invalid UTF-8 and a readable alternate. SHA-256 `940329d740b140d16d55d4a5415f76b0e04139b4c899637947eb37fbfade1421`.
- `invalid-script-with-header.s2ma` has the invalid primary and the binary title only. SHA-256 `42c3e5a610573b4e785d062fb91a6f37a65cb228f2bd4dd9a52fe5ec9e08e93f`.
- `script-reference-limit.s2ma` has readable title and author headings with 513 distinct bank references. SHA-256 `eb619969fb3f92fe7825bdaf727ace0fce7d4b332b83fde1e857da288a03feec`.
- `reference-limit-with-alternate.s2ma` keeps those primary headings, has a references-only alternate, and has no document header. SHA-256 `4cb317b1362456068450962d38a31b73bc3a23f1109a6f1c838210e6bd5591cd`.

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
bounded = '// Name: Bounded fixture headings\n// Author: Synthetic Studio\n'
bounded += ''.join(f'BankLoad("Fixture{index}", 1);\n' for index in range(513))
Path("bounded.galaxy").write_bytes(bounded.encode("utf-8"))
Path("references-only.galaxy").write_bytes(b'BankLoad("RecoveredBank", 1);\n')
Path("invalid.galaxy").write_bytes(b"// Name: Undecodable\n\xff\n")
Path("header.bin").write_bytes(b"DocInfo/Name.....\0Header fallback\x01\0")
PY
mkdir generated
./generate-fallback primary.galaxy alternate.galaxy header.bin generated invalid.galaxy bounded.galaxy references-only.galaxy
```
