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
Prepackaged builds are available in the
[Releases](https://github.com/Netdex/niinii/releases) section of this
repository.

The only target that is properly maintained is `x86_64-pc-windows-msvc`. There's
nothing stopping it from working on other targets (e.g. Linux), but additional
work may be required.

To build the application from source:
```
git clone https://github.com/Netdex/niinii.git
cd niinii
cargo build --release
```

For Japanese language support, the following additional components are required:
- ichiran-cli ([Ichiran](https://github.com/tshatrov/ichiran))
- PostgreSQL installation with Ichiran database

You must provide the location of these additional components in the Settings
pane of the application.

Given that the process of building these components for use with niinii is quite
involved, prebuilt versions are included with the prepackaged builds.

## Known Issues
### High CPU usage when out of focus
Seems like a problem with winit. niinii is almost always used in the foreground
anyways because of always on top, so I'm not going to bother fixing this.

### Hooking not working
- Most visual novels are written in engines which use D3D9. This is not always
  true though, you may need to adjust the hooking code as necessary.
- There is limited recovery code for when frame buffers are resized, devices
  are reset, contexts are changed etc. This may lead to breakages when
  switching in and out full-screen mode, resizing the window, and switching to
  another application.
- Some visual novel engines will present only when necessary rather than at a
  fixed framerate. In this case, niinii won't work properly since it expects a
  fixed refresh rate.

### Issues with Chromium-based browsers
In overlay mode, niinii displays a transparent window which covers the entire
screen. Newer Chromium-based browsers have a feature which suspends drawing
when the window is fully occluded, for performance reasons. The window
displayed by niinii counts as full occlusion despite being transparent, which
causes Chromium-based browsers to suspend drawing. I suspect this could also
happen with some Electron apps, but I haven't tested it.

## Troubleshooting
TODO

## Third-party
See NOTICE for a list of third-party software used in this project.