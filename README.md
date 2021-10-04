# niinii
`This project is a work-in-progress. (I pinky promise to actually finish this one)`

[![image](https://user-images.githubusercontent.com/2091886/124209159-04d6fa00-dab7-11eb-9ebf-32433e46db7c.png)](https://i.imgur.com/cmuYqq1.mp4)
[Demonstration](https://i.imgur.com/cmuYqq1.mp4)

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
- Style configuration
- Distribution

## Features
i.e. Completed roadmap items
- Hover over part-of-speech abbreviation to show explanation
- Interface and parser for ichiran-cli (implemented as `ichiran-rs` under `third-party\ichiran`)
- Visual representation of segmentation (RikaiView)
- Visual representation of gloss on term basis (TermView)
- Click hover window to open persistent window (which itself has hovers, for stuff like vias)
- Auto-clipboard
- Injection and D3D9 hook
- Automatic Postgres daemon

## Build
TODO

## Troubleshooting
TODO
