// The example catalog (examples/examples.toml) as site pages: an index of
// topics and a page per topic with every example, its poster (its first
// approved snapshot), its code and its source. The catalog and the scripts
// are read from the repository at build time, so a cataloged example shows
// up here without editing the docs.

#let repository = "https://github.com/PaoloLupo/gaanim"

#let catalog = toml(stdx.repo-file("examples/examples.toml"))

// The topics in reading order, each with its examples in catalog order.
#let topics = catalog.topic.map(topic => {
  let examples = catalog.example.filter(entry => entry.path.split("/").first() == topic.id)
  (..topic, examples: examples)
})

// The index groups the topics; showcase leads it on its own.
#let featured = "showcase"
#let blocks = (
  (title: "Foundations", topics: ("basics", "shapes", "style", "text", "math", "layout")),
  (title: "Motion", topics: ("animation", "transforms", "emphasis", "transitions", "camera")),
  (title: "Effects and 3D", topics: ("effects", "post_processing", "3d")),
  (title: "Data and science", topics: ("visualization", "reactivity", "mechanics")),
  (title: "Media and presentations", topics: ("audio", "media", "slides", "live")),
)

#for topic in topics {
  assert(
    topic.id == featured or blocks.any(block => topic.id in block.topics),
    message: "examples.toml topic " + topic.id + " has no block in docs/components/examples.typ",
  )
}

#let topic-route(topic) = "/ejemplos/" + topic.id + "/"

// The script path without `.py`: the example's id, and its snapshot folder
// under tests/visual/.
#let example-id(entry) = entry.path.slice(0, entry.path.len() - ".py".len())

// The file name, or the folder of a project's `main.py`.
#let example-slug(entry) = {
  let parts = example-id(entry).split("/")
  if parts.last() == "main" and parts.len() > 2 { parts.at(-2) } else { parts.last() }
}

#let example-title(entry) = entry.at(
  "title",
  default: {
    let words = example-slug(entry).replace("_", " ")
    upper(words.first()) + words.slice(1)
  },
)

#let example-anchor(entry) = "example-" + example-id(entry).replace("/", "-")

// The first approved snapshot: its site path, bytes and size.
#let poster(entry) = {
  let folder = "tests/visual/" + example-id(entry) + "/baseline/"
  let manifest = json(stdx.repo-file(folder + "manifest.json"))
  let first = manifest.snapshots.first()
  (
    path: "examples/" + example-id(entry) + ".png",
    data: stdx.repo-file(folder + first.file),
    width: manifest.width,
    height: manifest.height,
  )
}

#let poster-image(entry, image: none, class: "example-poster") = context {
  let image = if image == none { poster(entry) } else { image }
  html.elem("img", attrs: (
    class: class,
    src: stdx.config.asset-base + image.path,
    alt: example-title(entry),
    width: str(image.width),
    height: str(image.height),
    loading: "lazy",
    decoding: "async",
  ))
}

// Bundles up to this size play as soon as their example is on screen; a
// bigger one waits for a click before it downloads.
#let autoplay-limit = 2000000

// The example's poster with its bundle playing over it in the embed player,
// or only the poster when the build recorded no bundle.
#let preview(entry, image) = context {
  let bundle = stdx.example-bundle(entry.path)
  if bundle == none {
    return html.figure(class: "example-preview", poster-image(entry, image: image))
  }
  let base = stdx.config.asset-base
  let root = base.slice(0, base.len() - "assets/".len())
  let site = "bundles/examples/" + bundle.path.split("/").last()
  [#metadata((base + site, read("../" + bundle.path, encoding: none))) <metadata-asset>]
  // The player resolves the bundle from its own page, /reproductor/insertar/.
  let src = root + "reproductor/insertar/?src=../../assets/" + site + "&loop"
  html.figure(class: "example-preview", if bundle.bytes <= autoplay-limit {
    html.div(class: "anim-stage example-stage", {
      poster-image(entry, image: image)
      html.elem("iframe", attrs: (
        src: src,
        title: example-title(entry),
        loading: "lazy",
        allow: "fullscreen",
      ))
    })
  } else {
    let megabytes = str(calc.round(bundle.bytes / 1000000, digits: 1))
    html.elem("div", attrs: (class: "anim-stage example-stage", "data-player": src), {
      poster-image(entry, image: image)
      html.elem(
        "button",
        attrs: (type: "button", class: "example-play", "data-size": megabytes + " MB"),
        "Play · " + megabytes + " MB",
      )
    })
  })
}

#let count-label(n) = if n == 1 { "1 example" } else { str(n) + " examples" }

// Words the index filter matches a topic card against.
#let search-text(topic) = (
  (topic.title, topic.description)
    + topic.examples.map(entry => example-title(entry) + " " + entry.description)
).join(" ")

// Links from /ejemplos/ are relative to it, as the site may live under a
// base path.
#let card(href, search, body) = html.elem(
  "a",
  attrs: (href: href, class: "home-card topic-card", "data-search": lower(search)),
  body,
)

#let topic-card(topic) = card(
  topic.id + "/",
  search-text(topic),
  {
    poster-image(topic.examples.first(), class: "topic-card-poster")
    html.span(class: "home-card-eyebrow", count-label(topic.examples.len()))
    html.span(class: "home-card-title", topic.title)
    html.span(class: "home-card-body", topic.description)
  },
)

// The index of /ejemplos/: the featured topic's examples, then every block
// of topics as cards, with a filter. The book lists them instead.
#let examples-index() = context {
  if target() in ("bundle", "html") {
    let showcase = topics.find(topic => topic.id == featured)
    html.div(class: "examples-filter", {
      html.elem("input", attrs: (
        id: "examples-filter",
        type: "search",
        placeholder: "Filter topics and examples…",
        "aria-label": "Filter topics and examples",
        autocomplete: "off",
      ))
      html.span(class: "examples-filter-count", id: "examples-filter-count", "")
    })
    if showcase != none {
      html.section(class: "examples-block examples-featured", {
        heading(level: 2, numbering: none, showcase.title)
        html.p(class: "examples-block-body", showcase.description)
        html.div(class: "home-cards showcase-cards", {
          for entry in showcase.examples {
            card(
              showcase.id + "/#" + example-anchor(entry),
              example-title(entry) + " " + entry.description + " " + showcase.title,
              {
                poster-image(entry, class: "topic-card-poster")
                html.span(class: "home-card-title", example-title(entry))
                html.span(class: "home-card-body", entry.description)
              },
            )
          }
        })
      })
    }
    for block in blocks {
      html.section(class: "examples-block", {
        heading(level: 2, numbering: none, block.title)
        html.div(class: "home-cards topic-cards", {
          for topic in topics.filter(topic => topic.id in block.topics) {
            topic-card(topic)
          }
        })
      })
    }
  } else {
    for topic in topics [
      #heading(level: 2, numbering: none, topic.title)
      #topic.description
      #list(..topic.examples.map(entry => [*#example-title(entry)*: #entry.description]))
    ]
  }
}

// One example on its topic page: title, description, poster, links and code.
#let example-section(entry) = {
  let image = poster(entry)
  let source = str(stdx.repo-file("examples/" + entry.path))
  [#heading(level: 2, numbering: none, example-title(entry)) #label(example-anchor(entry))]
  html.p(class: "example-description", entry.description)
  preview(entry, image)
  context [#metadata((stdx.config.asset-base + image.path, image.data)) <metadata-asset>]
  html.div(class: "example-actions", {
    html.a(href: repository + "/blob/main/examples/" + entry.path, class: "example-action", "View on GitHub")
    html.code(class: "example-run", "gaanim examples/" + entry.path)
    if not entry.at("web", default: true) {
      html.span(
        class: "example-badge",
        title: "Uses video, Typst Universe packages or system fonts, which the web playground lacks",
        "Desktop app only",
      )
    }
  })
  html.details(class: "example-code", {
    html.summary("Code · " + str(source.split("\n").len()) + " lines")
    [#raw(source.trim(), lang: "python", block: true) <_stop>]
  })
}

// A topic's page body.
#let topic-page(topic) = {
  set heading(numbering: none)
  for entry in topic.examples {
    example-section(entry)
  }
}
