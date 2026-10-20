# Examples

<!-- Generated from examples.toml by `python scripts/examples.py readme`; do not edit. -->

Each example is a complete scene about one topic. It uses the default theme
and shows the variants the API offers for that topic, so it doubles as a
reference: the labels in the frame are the code that produced each variant.

Run one from a checkout with `just run <name>`, for example `just run easings`,
or open it with `gaanim examples/<topic>/<name>.py`.

## Topics

- [Basics](#basics)
- [Shapes](#shapes)
- [Style](#style)
- [Text](#text)
- [Math](#math)
- [Layout](#layout)
- [Animation](#animation)
- [Transforms](#transforms)
- [Emphasis](#emphasis)
- [Effects](#effects)
- [Post-processing](#post-processing)
- [Transitions](#transitions)
- [Camera](#camera)
- [3D](#3d)
- [Visualization](#visualization)
- [Reactivity](#reactivity)
- [Mechanics](#mechanics)
- [Audio](#audio)
- [Media](#media)
- [Slides](#slides)
- [Live audience](#live-audience)
- [Gallery](#gallery)
- [Showcase](#showcase)

## Basics

Scenes, objects, play and wait, groups and segments: the first things to learn.

| Example | Description |
| --- | --- |
| [First scene](basics/first_scene.py) | A first scene: objects on a stage, and play, wait and immediate changes moving the timeline cursor, drawn as a timeline. |
| [Scene tour](basics/scene_tour.py) | A tour of the scene: the factories that make shapes, text, equations, code, layouts, plots, SVGs and readouts, and the animations that change them. |
| [Groups](basics/groups.py) | Groups that move, turn and animate as one, nested arms that turn about their joints, and groups lined up or repeated. |
| [Segments](basics/segments.py) | Segments as chapters of a scene: notes, reuse, persistent objects, release, stops and markers. |

## Shapes

Geometry primitives, curves, arrows, strokes, boolean operations and clipping.

| Example | Description |
| --- | --- |
| [Primitives](shapes/primitives.py) | Every closed shape, line, arc, polyline and diagram mark that scene.geometry draws. |
| [Curves](shapes/curves.py) | Quadratic and cubic Bézier curves with their handles, composed curve commands and parametric polylines, one redrawn every frame. |
| [Arrows](shapes/arrows.py) | Straight, double and curved arrows with their sizes, grow_arrow against create, tips on any stroke and connectors that follow moving boxes. |
| [Strokes](shapes/strokes.py) | Stroke width and alignment, caps, joins and miter limits, dash patterns that march, tapered width profiles and trim paths. |
| [Boolean operations](shapes/boolean_operations.py) | Union, intersection, difference and xor of overlapping shapes, live results that follow their operands, fill rules and tolerance. |
| [Clipping](shapes/clipping.py) | Shapes, groups and text clipped to moving vector masks, inverted or with the even-odd rule, and fills that rise to a level from every side. |
| [Path modifiers](shapes/path_modifiers.py) | Zig zag, round corners, pucker and bloat, twist, wiggle and offset modifiers, animated, stacked and applied to text and trims. |

## Style

Colors, gradients, blend modes, themes and backgrounds.

| Example | Description |
| --- | --- |
| [Colors](style/colors.py) | CSS color strings, Color constructors, named constants, the theme's color roles and the Tailwind palette. |
| [Brushes](style/brushes.py) | Solid, linear, radial and sweep brushes, their extend modes, gradient text and strokes, and gradients that animate. |
| [Color maps](style/color_maps.py) | Built-in continuous and categorical color maps, sampling, custom, reversed and translucent maps, and data colored by value. |
| [Blend modes](style/blend_modes.py) | Every blend mode of a drawable over the same backdrop, and blended groups, member overrides and text. |
| [Themes](style/themes.py) | The palette of every built-in theme and its aliases, side by side on the same small composition. |
| [Custom themes](style/custom_themes.py) | A Theme derived from a built-in one with its own colors, fonts, text roles, style classes, spacing tokens and validation. |
| [Backgrounds](style/backgrounds.py) | The scene background, solid and gradient segment backgrounds, dot grids, mesh gradients, auroras, WGSL shader backgrounds and image backgrounds. |

## Text

Typography, code blocks, Typst layouts and animated text.

| Example | Description |
| --- | --- |
| [Typography](text/typography.py) | Text roles, style keywords, spacing, styled parts, inline markup and math, paragraph alignment and wrapping, anchors and selections. |
| [Code blocks](text/code_blocks.py) | Code panels in several languages, with highlighting off, a .tmTheme, theme color options, another monospaced font and different sizes. |
| [Text writing](text/text_writing.py) | write() by grapheme, word, line and part, in every order and stagger, and the chalk brush with its seed and roughness. |
| [Text reveals](text/text_reveals.py) | Masked slide, fade, scale and blur reveals by unit, blur-in and tracking, selection reveals, and the matching conceal and unwrite exits. |
| [Text animator](text/text_animator.py) | The text range animator: every selector shape, unit order, animated channel, unit type and sweep range. |
| [Typewriter](text/typewriter.py) | Typewriter speed, jitter, cursors, blinking and kept cursor, then backspace and retype edits of a terminal command. |
| [Scramble](text/scramble.py) | Scramble decoding with every charset, reveal delay, speed and seed, and scramble_to into new text. |
| [Text on path](text/text_on_path.py) | Text along closed and open paths with align, orient, reverse and offset, and path_offset animations that spin and slide it. |
| [Typst layouts](text/typst_layouts.py) | Typst documents as drawables: tables with merged cells and computed fills, grids and columns, aligned math, page widths and a .typ file. |
| [Typst packages](text/typst_packages.py) | Typst Universe packages in a scene: zebraw code listings and simple-plot function plots, styled with the theme's colors. |

## Math

Equations, step-by-step derivations, annotations and matrices.

| Example | Description |
| --- | --- |
| [Equations](math/equations.py) | Typst math notation, display and inline equations, quantities with units, sizes and aligned multi-line equations. |
| [Equation parts](math/equation_parts.py) | Named and nested equation parts, selections by name or fragment, per-term style and writing an equation part by part. |
| [Equation annotations](math/equation_annotations.py) | Emphasis, braces, leader-line notes and hand-drawn marks on single terms of an equation. |
| [Derivations](math/derivations.py) | Step-by-step derivations: structural transforms between equations, cancelled, revealed, copied and moved terms, and a worked solution. |
| [Matrices](math/matrices.py) | Matrix delimiters, row and column labels, spacing, cell content and entry and label styles. |
| [Matrix selections](math/matrix_selections.py) | Matrix row, column, block, diagonal and predicate selections, traversal orders and cell animations. |
| [Matrix edits](math/matrix_edits.py) | Inserting, removing, swapping and reordering matrix rows and columns, setting entries, and morphing one matrix into another. |
| [Matrix operations](math/matrix_operations.py) | Exact matrix sums, products, inverses, determinants and LU, QR and eigen decompositions computed with SymPy. |

## Layout

Boxes, rows, grids, zones and animated reflow.

| Example | Description |
| --- | --- |
| [Boxes](layout/boxes.py) | The box model: padding in every CSS shorthand, backgrounds, borders, radii, shadows, clipping, size rules and torn edges that fit their text. |
| [Box styles](layout/box_styles.py) | Reusable styles with BoxStyle, .but() and classes, typography inherited by box text, text_box and components. |
| [Flex](layout/flex.py) | Rows and columns with every justify and align value, gaps, wrapping, grow, shrink, basis, margins and absolute children. |
| [Grids](layout/grids.py) | Grid tracks in px, %, fr, auto and scene units, gaps, spans, explicit cells, column flow and alignment in cells. |
| [Stacks](layout/stacks.py) | Stacked layers placed by the nine anchors and offsets, every fit mode, and draw order with z_index. |
| [Zones](layout/zones.py) | Zones templates, nesting and inset, place() with anchor, padding, offset and fit, arrange(), fill() and animated moves between zones. |
| [Positioning](layout/positioning.py) | Free objects placed with to_edge, to_corner, next_to, align_to, move_to and linear layout constraints with priorities. |
| [Reflow](layout/reflow.py) | Animated structure changes: add, remove, replace, set, item, move_child, swap, reverse, detach, moves between boxes and reflow after a scale. |
| [Box reveal](layout/box_reveal.py) | Revealing a box piece by piece with reveal, cascade and stagger, and changing pieces with find, find_all and each. |
| [Scatter](layout/scatter.py) | Seeded scatter of formulas around a character's parts, with seeds, gaps, avoided zones and object regions. |

## Animation

Easing, keyframes, timing, motion paths and procedural motion.

| Example | Description |
| --- | --- |
| [Easings](animation/easings.py) | Every Easing preset, curve family, overshoot mode, spring and shaping factory, racing side by side. |
| [Entrances exits](animation/entrances_exits.py) | Every built-in entrance and exit: fades from each direction, stroke drawing, growth from a center, point or edge, and the exits that hide a drawable. |
| [Properties](animation/properties.py) | Every property target of drawable.animate: position, transforms, style, polygon points and custom animations computed by your own function. |
| [Keyframes](animation/keyframes.py) | Keyframes on every channel, an easing per segment between stops, and linear versus Catmull-Rom paths through the stops. |
| [Timing](animation/timing.py) | Repeats and loops, parallel, sequence and stagger, reshaping a composition's time, and staggered waves and distributed values from every origin. |
| [Choreography](animation/choreography.py) | Labels and relative inserts drawn as a timeline from Composition.schedule(), and animations cued on beats and bars of a tempo under a launched sweep. |
| [Motion paths](animation/motion_paths.py) | Travelling along circles, waves and arrows with move_along and its options, and straight moves bent into arcs with path_arc. |
| [Secondary motion](animation/secondary_motion.py) | Follow-through with settle, trailing followers, squash and stretch, and exact light physics with throw and inertia. |
| [Procedural motion](animation/procedural_motion.py) | Oscillators on every channel and waveform, wiggle, the other Updater presets, seeded randomness and smooth noise. |
| [Stepped time](animation/stepped_time.py) | Stop-motion time: drawables at several step rates, outlines and jitter that boil with hold=True, and a whole segment stepped, camera included. |

## Transforms

Morphing one object into another, matching parts and pivots.

| Example | Description |
| --- | --- |
| [Transform to](transforms/transform_to.py) | Morphs between shapes and their mirror images, open strokes, solid and gradient fills, transform_to against replacement and fade transforms, fading while morphing, and text to text, equation and shape. |
| [Matching](transforms/matching.py) | Matching transforms that pair pieces by shape, place and color, glyph matching between words and equations, and group and SVG morphs that pair parts by name or id. |
| [Magic move](transforms/magic_move.py) | magic_move by name, by SVG id and by a key function, fading or cutting unmatched members, and a leaderboard reordered and carried across a cut with Transition.magic_move. |
| [Pivots](transforms/pivots.py) | Rotations, scales and skews about a drawable's center, a scene point or a moved pivot, hinged groups, orbits and SVG parts, and shapes that aim at a moving target with look_at. |

## Emphasis

Drawing attention: indications, highlights and hand-drawn notes.

| Example | Description |
| --- | --- |
| [Indications](emphasis/indications.py) | Indicate, wiggle, blink, passing flashes, broadcast ripples, flashing frames and underlines, and the emphasis of one word inside a text. |
| [Highlights](emphasis/highlights.py) | Live surrounding frames that follow and retarget, color-cycling boundaries, highlighter marker bands and spotlights that dim everything else. |
| [Hand drawn](emphasis/hand_drawn.py) | Hand-drawn underlines, boxes, circles, strikes and brackets with every roughness, pass count, seed and style, marks that follow moving targets, and chalk shapes. |

## Effects

Per-object glow, blur, shadow, echo, mattes, shaders and particles.

| Example | Description |
| --- | --- |
| [Glow blur shadow](effects/glow_blur_shadow.py) | Glow, blur and drop shadow: their parameters, the drawables they apply to, and how they animate. |
| [Outlines](effects/outlines.py) | Sticker outlines traced from a drawable's alpha, on an image, text, a shape and a group, with a shadow and after a shader effect. |
| [Echo](effects/echo.py) | Trails of delayed copies with their count, delay and decay, what they replay, held trails and onion skins. |
| [Mattes](effects/mattes.py) | Track mattes in alpha and luma modes, inverted, animated and removed, and metaballs whose drops melt together. |
| [Glass](effects/glass.py) | Frosted and liquid glass over what is drawn behind, every refraction, bevel, twist and dispersion setting, and backdrop blur. |
| [Object shaders](effects/object_shaders.py) | WGSL passes on single drawables with uniforms, every PostProcess preset as an effect, chains and margins. |
| [Audio shaders](effects/audio_shaders.py) | Per-object shader effects driven by a music track's bands, level, pulses and spectrogram. |
| [Falloff](effects/falloff.py) | Falloffs that give each member of a group its own scale, turn, opacity, position or color: index, distance, noise, sources and their combinations. |
| [Repeaters](effects/repeaters.py) | Copies stepped by a transform with repeat, copies placed by a grid, circle, path or random distribution, and animated copy counts. |
| [Particles](effects/particles.py) | Particle emitter shapes, motion, looks, bursts and confetti, exact at any seek. |
| [Plexus](effects/plexus.py) | Live lines between drifting points, by range, nearest neighbors or sequence, with and without distance fading. |

## Post-processing

Full-frame passes: color grading, bloom, motion blur and finishes.

| Example | Description |
| --- | --- |
| [Color grading](post_processing/color_grading.py) | Exposure, contrast, saturation and temperature grades, a 3D lookup table, tint and duotone, applied to the whole frame. |
| [Finishes](post_processing/finishes.py) | Vignette, grain, chromatic aberration, halftone, dither, CRT, pixelate, glitch and image overlay passes, each ramped in through a Parameter, with labels kept out of the post-process. |
| [Bloom](post_processing/bloom.py) | Bloom threshold, intensity and radius on neon outlines, and a glow that pulses with a Parameter. |
| [Motion blur](post_processing/motion_blur.py) | Film-shutter motion blur that smears dots by speed, spinning, scaling and orbiting shapes, with one dot kept sharp, and segments with a shutter of their own. |
| [Post chains](post_processing/post_chains.py) | Chained passes in both orders, the scene post-process with segment overrides, and a custom WGSL pass whose uniform is a number, a Parameter or noise. |

## Transitions

Transitions between segments and content reused across them.

| Example | Description |
| --- | --- |
| [Cuts and fades](transitions/cuts_and_fades.py) | Cuts, cross-fades, fade-through and zoom-through, with flash and light-leak overlays and a transition sound. |
| [Reveals](transitions/reveals.py) | Vector reveals: wipes, clock wipes, iris shapes, blinds, push and slide, with their directions and a spring easing. |
| [Morph transitions](transitions/morph_transitions.py) | Drawables that travel across the cut with Transition.morph pairs declared through scene.link, and Transition.magic_move by name. |
| [Shader blends](transitions/shader_blends.py) | Shader transition presets with their settings, a luma reveal from a grayscale image, and a custom WGSL swirl with uniforms. |
| [Segment reuse](transitions/segment_reuse.py) | Content across segments: reuse through a cross-fade, a persistent badge, release, and a transition added later with scene.link. |

## Camera

Pan, zoom and rotation, camera paths, insets and parallax.

| Example | Description |
| --- | --- |
| [Camera moves](camera/camera_moves.py) | Pan, zoom and rotate the camera, frame objects with margins, and follow a moving target. |
| [Camera states](camera/camera_states.py) | Reusable camera poses: state_2d, capture, to, and named save and restore. |
| [Camera shots](camera/camera_shots.py) | Drive the camera along a path, whip pan with motion blur, and shake it with the trauma model. |
| [Camera bindings](camera/camera_bindings.py) | Bind the camera's center, zoom and rotation to drawables and reactive values, blended by influence and switched on and off. |
| [Camera insets](camera/camera_insets.py) | Insets that enlarge a detail in a screen joined to it: placement, shapes, view moves, following and fixed screens. |
| [Camera views](camera/camera_views.py) | Shapes that show what a second camera sees: frames, fit modes, lenses, X-ray view layers and snapshots. |
| [Parallax](camera/parallax.py) | Parallax layers at several depths under pan, zoom and rotation, and depth of field with a rack focus. |

## 3D

Primitives, arrows and tubes, extrusion, axonometric drawings and 3D axes.

| Example | Description |
| --- | --- |
| [Primitives 3d](3d/primitives_3d.py) | Cubes, spheres, cylinders, cones and planes as lit meshes, with their segments, rings and caps. |
| [Materials 3d](3d/materials_3d.py) | Matte, metal and emissive materials, roughness, metallic and emission settings, and animated materials and fills. |
| [Transforms 3d](3d/transforms_3d.py) | Turning, moving and scaling drawables in depth with the _3d methods, about their center or a pivot. |
| [Extrusion](3d/extrusion.py) | Text, equations, SVGs, shapes with holes and groups extruded into lit solids, with depth, bevel, tolerance and materials. |
| [Shapes 3d](3d/shapes_3d.py) | Tori, arrows between two points, tubes along open, creased and closed paths, flat sheets of text, equations and shapes, and lit lines between points or the drawables they follow. |
| [2D drawables in 3D](3d/drawables_in_3d.py) | Text, equations and shapes placed in the 3D world with in_3d(): turned, written, tilted by perspective, hidden by meshes and lit, beside billboard(). |
| [glTF models](3d/models_3d.py) | A glTF model imported with scene.media.gltf: its rest pose, its animations played one after another with fades, and loops, speeds and reversed plays. |
| [Glow on 3D content](3d/glow_3d.py) | A halo around what the camera sees of a sphere, a 3D line and a walking glTF model, grown and faded with animate.glow, and hidden where a solid in front covers it. |
| [Lines 3d](3d/lines_3d.py) | Polylines through 3D points with solid colors and color maps, and paths traced behind moving objects that fade, cap or thin. |
| [Axes 3d](3d/axes_3d.py) | 3D axes with their grid planes, box sizes, linear, log, symlog and power scales, layers that animate apart, and planes and axes that take the sides the camera shows. |
| [Plots 3d](3d/plots_3d.py) | Surfaces, parametric curves and vector fields in a 3D space, static and driven by parameters. |
| [Axonometric](3d/axonometric.py) | A 3D model drawn as flat shapes from isometric, dimetric, plan, front and side views, turned and morphed between views. |
| [Camera 3d](3d/camera_3d.py) | The 3D camera: look_at, orbit, dolly and dolly zoom, perspective and orthographic lenses, saved poses, state_3d, bind_3d, a camera and an inset that follow a moving sphere, a camera that turns to follow it, a snapshot of the inset, and a view with a 3D camera of its own. |

## Visualization

Axes, plots, charts, vector fields and live readouts.

| Example | Description |
| --- | --- |
| [Axes](visualization/axes.py) | Linear, log, symlog, power, category and time axes, tick steps, number formats, styles, titles and crossings. |
| [Coordinate spaces](visualization/coordinate_spaces.py) | Cartesian, polar and complex planes, number lines, grids and numbers, coordinates that place objects and animated views. |
| [Plots](visualization/plots.py) | Function, parametric and implicit curves, log axes, Riemann sums and other calculus constructions, and curves that follow parameters. |
| [Chart marks](visualization/chart_marks.py) | Every chart mark: points, lines, steps, areas, bars, histograms, box plots, violins, error bars, heatmaps, pies, donuts and roses, and marks that enter with a wipe. |
| [Chart encodings](visualization/chart_encodings.py) | Color, size, opacity and label channels, scales, categorical palettes, legends and color bars, and explicit axes. |
| [Chart transitions](visualization/chart_transitions.py) | Charts that morph between specs by key or index, change mark family or data, grow from 2D into 3D, and pies that share out their slices anew. |
| [Bar race](visualization/bar_race.py) | Ranked bars that grow, overtake each other and leave the top, with value formats, colors and a rolling ticker. |
| [Vector fields](visualization/vector_fields.py) | Vector fields as arrows and streamlines with their options, flowing highlights, particles, advection and color maps. |
| [Readouts](visualization/readouts.py) | Rolling counters in every format and mode, live readouts of parameters, functions and time, progress rings and countdowns. |

## Reactivity

Parameters, computed values, updaters and objects that follow each other.

| Example | Description |
| --- | --- |
| [Parameters](reactivity/parameters.py) | Parameters animated, cut and keyframed, a Variable that shows itself, computed values and chains, every setter that follows a source, and what ends a binding. |
| [Updaters](reactivity/updaters.py) | Per-frame Python updaters, stateless and simulated, sampled series on every channel, stacked and removed presets, and arcs redrawn from a Parameter. |
| [Connectors](reactivity/connectors.py) | Reference points, followers, coordinate and rotation bindings, look_at, and lines, labels and frames that keep up with moving objects. |
| [Curve trackers](reactivity/curve_trackers.py) | A point, tangent, normal and osculating circle riding polylines, Bézier curves, arcs and polygons, and traced paths that fade, cap or thin their samples. |
| [Circle to sine](reactivity/circle_to_sine.py) | One angle turns a point on a circle while traced paths draw its sine and cosine waves, with live readouts. |

## Mechanics

Mechanisms, transmissions, supports, forces and simulations.

| Example | Description |
| --- | --- |
| [Supports](mechanics/supports.py) | Every support kind, the directions they face, their sizes and their styleable parts. |
| [Dimensions](mechanics/dimensions.py) | Linear dimensions with symbols and readouts, the side they sit on, their units, formats, style and parts. |
| [Angle dimensions](mechanics/angle_dimensions.py) | Angles between rays with symbols and readouts, in every sweep, with their arrowheads, extensions and labels. |
| [Forces](mechanics/forces.py) | Force vectors with readouts, forces from a magnitude or from components, moments, coordinate frames and a free body diagram. |
| [Mechanisms](mechanics/mechanisms.py) | Bars and joints between moving points, a slider-crank and a four-bar linkage with a traced coupler point. |
| [Springs](mechanics/springs.py) | Helical springs between moving endpoints, with their coils, amplitude, crossings and straight ends, in chains and on levers. |
| [Transmissions](mechanics/transmissions.py) | Gears and their options, gear trains, racks and pinions, cams with followers and contact on curves. |
| [Simulations](mechanics/simulations.py) | A damped pendulum, a double pendulum and a mass on a spring, stepped at a fixed rate and exact at any seek. |

## Audio

Sound effects, music-reactive animation and narration.

| Example | Description |
| --- | --- |
| [Music](audio/music.py) | Audio clips played at the cursor with their length, volume and fades, and animations cut on the beats and bars of a track's tempo. |
| [Sound effects](audio/sound_effects.py) | Sounds anchored to an animation with volume, delay and offset, sounds placed at a second, and sounds played by transitions and staggers. |
| [Signals](audio/signals.py) | A track's loudness, bands, hits and beats as signals, and the sizes, opacities, updaters, falloffs and readouts they drive. |
| [Visualizers](audio/visualizers.py) | Equalizer shapes, layouts and fills, and the spectrum, waveform and samples of a track they draw. |
| [Narration](audio/narration.py) | Voiceover blocks timed by their takes, markers in the text, a narration script and a live take recorded while presenting. |

## Media

Images, SVG, video and Lottie animations.

| Example | Description |
| --- | --- |
| [Images](media/images.py) | Raster images fitted to a frame in every mode, cropped by pixels or fractions, panned and zoomed by animating the crop, sampled at every quality, and points and regions that stay on their pixels through a zoom. |
| [Svg](media/svg.py) | SVG files imported with their gradients, clip paths and filters, drawn in with write, create and fade_in, and parts addressed by id, restyled and animated. |
| [Video](media/video.py) | MP4 clips synchronized with the timeline: fitted and cropped, trimmed, looped, sped up and slowed down, and cut into segments. |
| [Lottie](media/lottie.py) | Lottie JSON clips trimmed, looped and sped up, drawn in before they play, and a dotLottie state machine driven by inputs, events and themes. |

## Slides

Presentations: slides, editorial components, sections and tables.

| Example | Description |
| --- | --- |
| [Slides](slides/slides.py) | A short deck of segments with slide templates, steps that stop, speaker notes, transitions and a looping stop. |
| [Sections](slides/sections.py) | Agendas in every layout and marker, progress rails in rows and columns with captions, and sections of slides that move them. |
| [Editorial titles](slides/editorial_titles.py) | Title cards, section headers, banners and lower thirds with their alignments, widths, variants and appearances. |
| [Editorial components](slides/editorial_components.py) | Badges and chips in every appearance and variant, cards, bullet lists and callouts that point at objects. |
| [Tables](slides/tables.py) | Slide tables with their widths, row heights and alignment, and data tables built from layout rows whose cells, rows and columns animate. |
| [Branding](slides/branding.py) | A deck theme derived from presentation, with a logo, footer and slide number placed on every slide but the cover. |

## Live audience

Polls, quizzes and live zones the audience joins from their phones.

| Example | Description |
| --- | --- |
| [Lobby](live/lobby.py) | The room the audience joins before the first question: its QR code and room code, slots that fill as people join, names and join times. |
| [Polls](live/polls.py) | A poll for the audience's phones and its live data: bars in every direction and scale, votes, shares, icons and colors, and rehearsal votes. |
| [Quizzes](live/quizzes.py) | Quizzes with a right answer, a countdown and a reveal, rehearsed answers, scoring, timing and questions loaded from Markdown with images. |
| [Leaderboards](live/leaderboards.py) | Players ranked by points with their names, bars, points, correct answers, streaks and a podium. |
| [Teams](live/teams.py) | The audience split into teams whose points add up, with team scores, averages, bars, leaders and a tug of war. |
| [Characters](live/characters.py) | The characters the audience builds on their phones: every body, eyes, mouth, color and extra, expressions and named characters. |
| [Live zones](live/live_zones.py) | Live zones that pose the audience's characters every frame: position, rotation, scale, squash, lean, looks, expressions and names. |
| [Zone behaviors](live/zone_behaviors.py) | The motion helpers of gaanim.live, from lerp and easings to springs, wobbles, impacts and ballistic arcs, and state a zone keeps per player. |
| [Zone games](live/zone_games.py) | Characters that move with the game: grouped by the answer each player chose, racing by score, and standing on a podium by rank. |

## Gallery

Short scenes in the brand's colors, from a logo built of primitives to Fourier epicycles, each one a classic of animation galleries made with the whole API.

| Example | Description |
| --- | --- |
| [Logo](gallery/logo.py) | The logo built from primitives: a square that turns into a circle as it moves, and a wordmark cut from ellipses and bars. |
| [Measured segment](gallery/measured_segment.py) | A segment between two points whose run, rise and length are measured live while the points move. |
| [Vector arrow](gallery/vector_arrow.py) | A vector on a plane whose components, angle and coordinates follow it as it sweeps around, leaving a fading trail. |
| [Boolean shapes](gallery/boolean_shapes.py) | Two ellipses and their union, intersection, exclusion and difference, which reshape live as one ellipse turns and slides. |
| [Point on shapes](gallery/point_on_shapes.py) | Points that travel along a circle, a square and a star with a trail of echoes, and an arm that turns about its end. |
| [Moving around](gallery/moving_around.py) | A square that slides, recolors, shrinks and turns, one call at a time and then all at once, squashing as it speeds. |
| [Moving angle](gallery/moving_angle.py) | An angle that opens and closes while its arc, its value in degrees and a fading trail follow the moving ray. |
| [Sliding ladder](gallery/sliding_ladder.py) | A ladder whose feet slide along a wall and the floor, its midpoint tracing a quarter circle and its past positions outlining an astroid. |
| [Group to destination](gallery/group_to_destination.py) | A cluster of points that travels so its gold leader lands on each target, the others trailing it on delayed followers. |
| [Moving frame box](gallery/moving_frame_box.py) | The product rule written term by term, with a frame and a band that glide from one term to the next. |
| [Point with trace](gallery/point_with_trace.py) | A pen on the end of two turning arms that traces a rose, the second arm turning four times faster the other way. |
| [Sine cosine plot](gallery/sine_cosine_plot.py) | The sine and the cosine over two turns, with a scanning line that reads both curves until it stops at two pi. |
| [Gradient descent](gallery/gradient_descent.py) | Gradient descent from two starts on the same curve: one ball settles in a shallow valley, the other finds the deepest one. |
| [Riemann area](gallery/riemann_area.py) | Riemann sums that refine from four rectangles to sixty-four under a curve, their total closing in on the area. |
| [Hyperbola rectangle](gallery/hyperbola_rectangle.py) | A rectangle from the origin to a point that slides along the hyperbola y = 6/x, changing shape while its area stays six. |
| [Heating curve](gallery/heating_curve.py) | The heating curve of water, traced as a thermometer fills, through melting and boiling. |
| [Following camera](gallery/following_camera.py) | A camera that zooms in on a point riding a sine wave, follows it along two turns and pulls back out. |
| [Zoomed inset](gallery/zoomed_inset.py) | A Koch snowflake explored through a round magnifier that follows a point along its edge and zooms deeper. |
| [Surface plot](gallery/surface_plot.py) | A Gaussian hill on 3D axes that the camera circles while the hill narrows into a peak and spreads back out. |
| [Light source](gallery/light_source.py) | A gold point light that sweeps over three solids like a sun, lighting each from the side it passes. |
| [Camera orbit](gallery/camera_orbit.py) | Lit solids on 3D axes seen by a camera that circles them, tilts up and down, and keeps a readout fixed on the screen. |
| [Catenoid helicoid](gallery/catenoid_helicoid.py) | A helicoid that bends into a catenoid without stretching, its curvature painted on the material and two curves keeping their lengths. |
| [Lorenz attractor](gallery/lorenz_attractor.py) | Eight paths of the Lorenz system that start a ten-thousandth apart, travel as one curve, then split and each weave the butterfly its own way. |
| [Heavy top](gallery/heavy_top.py) | Three identical spinning tops under glass, tilted alike and let go three ways: none of them falls, each precesses and nods between two circles. |
| [Warped grid](gallery/warped_grid.py) | A sentence and an equation that write themselves and transform, then a grid bent by a nonlinear function. |
| [Fourier epicycles](gallery/fourier_epicycles.py) | Five turning circles stacked on each other, the odd harmonics of a square wave, whose tip draws the wave as they spin. |

## Showcase

Complete pieces that combine many topics, each with its own look.

| Example | Description |
| --- | --- |
| [Dashboard](showcase/dashboard.py) | An analytics dashboard built only from boxes, revealed piece by piece and updated live. |
| [Lower third](showcase/lower_third.py) | A broadcast lower third, a live badge and a news ticker over drifting footage. |
| [Mobile app](showcase/mobile_app.py) | A phone chat app built from boxes: a clipped device, bubbles that wrap and a composer that types. |
| [Quiz night](showcase/trivia/main.py) | A live trivia played in teams: a room the audience smashes into, polls and quizzes from Markdown, reveals with confetti, a tug of war, a score race and a podium. |
