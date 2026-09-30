"""Ask the audience between slides: `gaanim --present audience_poll_demo.py`.

Set up the relay once with `gaanim relay init` and `gaanim relay use <URL>`;
at each poll the audience screen shows a QR code and the live votes.
"""

from gaanim import BLACK, GOLD, GRAY, WHITE, Scene, title_slide

scene = Scene(frame=(16, 9), background=BLACK, margin=0.533333)

opening = scene.segment("Warm-up", notes="Ask before explaining.", template=title_slide)
opening.bind(
    title=scene.text("Which curve grows fastest?", role="title").fill(GOLD),
    subtitle=scene.text("Scan the code and vote", role="subtitle").fill(WHITE),
)
scene.wait(0.45)
scene.poll("Which curve grows fastest?", ["x²", "2ˣ", "x log x"], name="first-guess")

answer = scene.segment("Answer", notes="Reveal, then ask again.", template=title_slide)
answer.bind(
    title=scene.text("2ˣ wins, eventually", role="title").fill(GOLD),
    subtitle=scene.text("Exponentials outgrow every polynomial", role="subtitle").fill(WHITE),
    footer=scene.text("Did the explanation help?").fill(GRAY),
)
scene.wait(0.45)
scene.poll("Did the explanation help?", ["Yes", "Somewhat", "No"], name="feedback")

scene.render()
