# Synthetic MPQ fixtures

StormLib v9.40 created these archives from self-authored text. Its reader reopened each archive and compared every available member with the input byte for byte. The same text is reconstructed in the Rust test. The synthetic `DocumentHeader` member tests archive extraction only. Its payload is intentionally plain text. [StormLib release](https://github.com/ladislav-zezula/StormLib/releases/tag/v9.40)

| Fixture | Coverage | SHA-256 |
| --- | --- | --- |
| `v1-no-list.s2ma` | Classic tables, uncompressed and single-unit/multi-sector zlib metadata, no listfile | `bbf6555dd7560c2b925c6662725dee358507a00ad9b2bf15c0c1cab30205ca14` |
| `v1-list.s2ma` | Bzip2 script, zlib alternate script, missing document header, listfile present | `2531fff6255fb1fd4cf2bf93a2b5729613966a7fc0529a45481c8fdb479c7373` |
| `v2-list.s2ma` | V2 header, listfile present | `314567ac7f1872a00a3a151f0d337c4d55a7e63c2bdd6c2f83db047a8b2daa8d` |
| `v4-no-list.s2ma` | V4 archive with classic and extended tables, no listfile | `06acda8f416276e899451b9a9efd13eba3cf034ec42e3084d940873678ff813d` |

Run the generator in a fresh output directory after verifying the release source. The commands below assume a checkout named `StormLib-9.40` and the full path to the fixture directory in `$fixture_dir`.

```sh
cmake -S StormLib-9.40 -B storm-build -DCMAKE_BUILD_TYPE=Release \
  -DSTORM_SKIP_INSTALL=ON -DSTORM_BUILD_TESTS=OFF -DSTORM_USE_BUNDLED_LIBRARIES=ON
cmake --build storm-build
c++ -std=c++17 -Wall -Wextra -Werror -IStormLib-9.40/src "$fixture_dir/generate.cpp" \
  storm-build/libstorm.a -o generate-fixtures
python3 - <<'PY'
from pathlib import Path

payload = "// Name: Synthetic – alpha\n// Author: Fixture only\n"
payload += "// Content for sector coverage.\n" * 320
Path("payload.txt").write_bytes(payload.encode("utf-8"))
PY
mkdir generated
./generate-fixtures payload.txt generated
```
