# Bundled fonts

Both families are under the SIL Open Font License 1.1. Each family's licence
file sits next to the fonts in `crates/analyte/assets/fonts/` and is built into
the program and served with them at `/static/fonts/`. The file names are the
ones `assets/fonts.css` names. Both projects publish static `.woff2` files, so
nothing was converted.

## Instrument Sans (400, 500, 600, 700)

- Source: the official repository, github.com/Instrument/instrument-sans, at
  commit `7fa22308a3d0c94ee2b3cd537a1196b65db34a3e`, files
  `fonts/webfonts/InstrumentSans-{Regular,Medium,SemiBold,Bold}.woff2` and
  `OFL.txt` (shipped as `OFL-InstrumentSans.txt`).
- Fetched with `curl -sSfL https://raw.githubusercontent.com/Instrument/instrument-sans/<commit>/<path>`.

| File | SHA-256 |
| --- | --- |
| InstrumentSans-Regular.woff2 | `f28af62faa9eec1e5482cf6e2a3e06bc865fa2fa6937bd56c14d2be23c9b4c46` |
| InstrumentSans-Medium.woff2 | `65d25fc111c40fd6b481d9db860af365730752228dfd8a246e79648d8a01f02a` |
| InstrumentSans-SemiBold.woff2 | `04235f235483213906d69cafb7a87ee197adaffacb081c8a7f809d00f9b353cd` |
| InstrumentSans-Bold.woff2 | `3e67cdb08813ada42a8a58190c2082f60862ec84bf3d2a5033002d8475efc8da` |
| OFL-InstrumentSans.txt | `9e27a72ed30eb49a08678f6a5d6ed98ec7ba5368f541637ee0683ec9134ef966` |

## IBM Plex Mono (400, 500, 600)

- Source: IBM's Plex release `@ibm/plex-mono@2.5.0` (github.com/IBM/plex,
  published 2026-06-11), asset `ibm-plex-mono.zip`, SHA-256
  `6d23f01257663d8cc49a0d64c22ced630b79e0e2a0ac08a0da86e9a38bbc481c`.
- Fetched with `gh release download "@ibm/plex-mono@2.5.0" -R IBM/plex -p ibm-plex-mono.zip`;
  the files below were extracted unchanged from
  `ibm-plex-mono/fonts/complete/woff2/` and `ibm-plex-mono/LICENSE.txt`
  (shipped as `OFL-IBMPlexMono.txt`). The licence is published with CRLF line
  endings; it is shipped with LF endings, because the repository refuses CR,
  and its text is otherwise unchanged (published SHA-256
  `7e6b2818edbd8f6a01ae80641cc8f16a51080d08fb4e532be3a0b6f74adb07da`).

| File | SHA-256 |
| --- | --- |
| IBMPlexMono-Regular.woff2 | `ba204497f16b6d334cee9d1e963a831b73e3a56e1d6300a8489d18df7214b350` |
| IBMPlexMono-Medium.woff2 | `33faf307fa6031fb4062276d7320a6d632de890cbb347576fd80cfa01077bc25` |
| IBMPlexMono-SemiBold.woff2 | `6a825b4824c01cbb401e829e5a066a1818411bcb3538b5a5792c5ca9b82343c3` |
| OFL-IBMPlexMono.txt | `37784b44044a4ffd9256702b7c0982c37e5c8887ba90c6dca0479aea93dc898d` |
