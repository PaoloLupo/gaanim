# Quiz Night

A live general-knowledge trivia played in teams (Owls against Larks) that
brings together what Gaanim offers for games with an audience: a waiting
room with characters, questions with images and multiple answers written in
Markdown, reveals with `magic_move` and confetti, a battle between teams, a
score race and a podium.

- The questions are in `questions.md`: each `#` is a question, `[x]` marks
  the right answers (several = multiple choice, none = poll) and the lines
  `time:`, `right:`, `votes:`, `notes:` and `![](image)` tune each one.
  `main.py` builds the slides by the kind of question.
- `python tools/make_images.py` draws the images.
- The sounds in `assets/sounds` are by [Kenney](https://www.kenney.nl)
  (Interface Sounds and Music Jingles packs, CC0 license) and the font is
  Fredoka (SIL Open Font License, in `assets/fonts/OFL.txt`).
- In the preview a made-up class of 18 students plays (`scene.rehearsal`),
  so the room, the answers and the podium show without phones.

## Preview and present

With a relay set up (`gaanim relay`, see the *Live audience* guide):

```sh
gaanim examples/showcase/trivia
gaanim --present examples/showcase/trivia
```

To present it without Python, record it into a bundle:

```sh
gaanim export examples/showcase/trivia --output trivia.gaanim
gaanim --present trivia.gaanim
```
