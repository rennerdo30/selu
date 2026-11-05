Local CASC reader

casc-lib 0.2.1 (MIT, Xerrion) was evaluated and rejected because its
unconditional reqwest dependency cannot be disabled. No casc-lib dependency
is retained. src/hash.rs adapts its Jenkins hash implementation, correcting
the word order in the path hash. src/local.rs uses its format parsers as a
reference. Upstream: https://github.com/Xerrion/casc-extractor
See LICENSE-casc-lib for the retained MIT notice.

BLP2 decoder adapted from Selu SIM2's selu-debug/src/textures.rs (MIT).
The storage reader performs only local reads and never opens WTF or account
files. Paths resolve through root hashes or an explicitly supplied listfile.
Missing content fails locally; there is no network fallback.
CLI examples:

selu gamedata --install "E:/Games/Battle.net/World of Warcraft" --product wowf info
selu gamedata --install "E:/Games/Battle.net/World of Warcraft" --product wowf cat "Fonts/FRIZQT__.TTF"
selu gamedata --install "E:/Games/Battle.net/World of Warcraft" --product wowf atlas "UI-Character-Info-Title"

cat writes raw bytes to stdout. --listfile accepts an explicit local semicolon
separated ID/path file. No listfile is bundled or downloaded. DB2 filenames
have no root hashes in Forever; db2::TABLES lists the two schema-sourced IDs.
The inspect example additionally writes decoded PNGs or extracted bytes to an
explicit output path. Never choose an output beneath the game install.

Supported: local v7 indices, EN v1 encoding, MFST V1/V2 roots, raw/zlib BLTE,
BLP2 palette/BGRA/DXT mip zero, and WDC5 fixed uncompressed records for the
verified Forever atlas layouts. Other WDC layouts, sparse/encrypted sections
and encrypted BLTE refuse explicitly; the proof files did not need encryption.
The cache is in LOCALAPPDATA/Selu/gamedata/<hex product>/<build config hash>/,
with content-key filenames. Both cached and decoded bytes are checked against
the root MD5 content key. Interrupted writes are detected on the next read.
Root and encoding metadata are reopened per Storage instance; no simulator or
Studio resolver wiring is included.

Schema references (identifiers and layout definitions, no downloaded game data):
https://github.com/wowdev/WoWDBDefs/blob/master/manifest.json
https://github.com/wowdev/WoWDBDefs/blob/master/definitions/UiTextureAtlas.dbd
https://github.com/wowdev/WoWDBDefs/blob/master/definitions/UiTextureAtlasMember.dbd
