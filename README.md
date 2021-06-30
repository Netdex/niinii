# niinii
`This project is a work-in-progress. (I pinky promise to actually finish this one)`
![image](https://user-images.githubusercontent.com/2091886/123896293-48066100-d92f-11eb-9bd6-aebdac5ad932.png)

Graphical frontend for glossing based on data from [ichiran](https://github.com/tshatrov/ichiran). 
This is a tool created to service a personal need, and may not be useful to you.
Below, I laid out my personal justification for investing time into creating
this tool. If you agree, then this tool may be useful for you.

**Why not use MeCab, JParser, ChaSen, etc.?**: In my experience ichiran is
much better at segmentation, provides more metadata, and makes fewer mistakes.

**Why not use rikai(kun|chan), JGlossator, Jisho?**: They don't do segmentation, 
they're not even morphological analyzers.

**Why not use DeepL, Google Translate, etc.?**: I want a gloss for language
learning purposes, not a translation tool.

**Why not use the web frontend [ichi.moe](https://ichi.moe)?**: 
- There are a number of additional features I would like to implement
	- Some of these features are not possible on a web platform
	- The source code is not available as of writing
- Optimally, I would like to be able to use this tool offline

## Roadmap
- Interface and parser for ichiran-cli (implemented as `ichiran-rs`)
- Visual representation of segmentation
- Visual representation of gloss on term basis (tooltip)
- Auto-clipboard
- Low-barrier distribution (i.e. extract and run)

TODO

## Build
TODO

## Troubleshooting
TODO
