# niinii
`This project is a work-in-progress. (I pinky promise to actually finish this one)`

[![image](https://user-images.githubusercontent.com/2091886/136855310-728670e8-706e-40e8-8b60-1a520ac7b44d.png)](https://streamable.com/rpvc9s)

[Demonstration](https://streamable.com/rpvc9s)

Graphical frontend for glossing based on data from [ichiran](https://github.com/tshatrov/ichiran). 
This is a tool created to service a personal need, and may not be useful to you.
Below, I laid out my personal justification for investing time into creating
this tool. If you agree, then this tool may be useful for you.

**Why not use MeCab, JParser, ChaSen, Jisho etc.?**: In my experience ichiran is
much better at segmentation, provides more metadata, and makes fewer mistakes.

**Why not use rikai(kun|chan), JGlossator?**: They don't do segmentation.

**Why not use DeepL, Google Translate, etc.?**: I want a gloss, not a translation tool.

**Why not use the web frontend [ichi.moe](https://ichi.moe)?**: 
There are some features I'd like to experiment with to improve the glossing experience.

## Roadmap
- Display omitted segments
- Style configuration
- Distribution

## Features
i.e. Completed roadmap items
- Visual representation of segmentation
- Interface and parser for ichiran-cli (implemented as `ichiran-rs` under `third-party\ichiran`)
- Postgres daemon
- Clipboard capture
- Injectable DLL with D3D9 hook
- Ruby text (furigana, romaji)
- Kanji lookup tooltip
- Cycle through segment variations
- Persistent vocabulary windows
- Fullscreen overlay

## Build
TODO

## Known Issues
### High CPU usage when out of focus

## Troubleshooting
TODO
