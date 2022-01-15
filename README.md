# niinii
`This project is a work-in-progress.`

[![image](https://i.imgur.com/9sDcn8v.png)](https://www.youtube.com/watch?v=Z6Aj2BYqklg)

[Demonstration](https://www.youtube.com/watch?v=Z6Aj2BYqklg)

niinii (knee-knee) is a graphical frontend for glossing Japanese text. Useful
for assisted reading of text for language learning purposes. A primary use case
is glossing visual novels, which is shown in the demonstration above. I made
this tool with the express intent to read a single *specific* visual novel,
which is also where the name comes from. If someone else finds it useful that's
cool too.

For example, in the demonstration above, I use niinii along with a text hooker
to gloss the dialogue in a visual novel. The segmented phrase along with ruby
text (i.e. furigana) is displayed. Hovering over a segment will show dictionary
definitions and inflections from JMDict. You can pop open a separate window by
clicking on a segment. Hovering over kanji will show kanji information from
KANJIDIC2. I would write a more detailed user manual but I think you can
probably figure it out.

Japanese language support is implemented using
[Ichiran](https://github.com/tshatrov/ichiran) by
[tshatrov](https://github.com/tshatrov). Ichiran is pretty amazing at text 
segmentation compared to other tools I've tried.

## Why not use...
This is a tool created to service a personal need, and may not be useful to you.
Below, I laid out my personal justification for investing time into creating
this tool. If you agree, then this tool may be useful for you.

**Why not use MeCab, JParser, ChaSen, Jisho etc.?**: In my experience ichiran is
much better at segmentation, provides more metadata, and makes fewer mistakes.

**Why not use rikai(kun|chan), JGlossator?**: They don't do segmentation.

**Why not use DeepL, Google Translate, etc.?**: I want a gloss, not a
translation tool. If I ever integrate translation features, I'd like to do so in
a way that supplements the gloss rather than dumping text.

**Why not use the web frontend [ichi.moe](https://ichi.moe)?**: 
There are some features I'd like to experiment with to improve the glossing
experience.

## Build
TODO

## Known Issues
### Missing characters when rendering (displays as <?>)
ImGui uses a static font atlas for rendering text. niinii bakes an atlas with
only ~3000 of the most common Japanese kanji, because any more and the texture
will become too large. This means all hyōgai kanji will not render. In the
future, we can dynamically rebuild the font atlas when encountering new kanji.

### High CPU usage when out of focus
Seems like a problem with winit. niinii is almost always used in the foreground
anyways because of always on top, so I'm not going to bother fixing this.

## Troubleshooting
TODO

## Third-party
See NOTICE for a list of third-party software used in this project.