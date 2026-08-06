#context asset(stdx.config.asset-base + "script.js", read("script.js"))

#context asset(
  stdx.config.asset-base + "base.css",
  read("base.css").replace(
    "url(\"/assets/fonts",
    "url(\"fonts",
  ),
)

#let fonts = (
  "Aleo-Italic-VariableFont.ttf",
  "Aleo-VariableFont.ttf",
  "VictorMono-Italic-VariableFont.ttf",
  "VictorMono-VariableFont.ttf",
  "NewCMMath-Regular.otf",
)

#context for filename in fonts {
  let data = read("fonts/" + filename, encoding: none)
  asset(stdx.config.asset-base + "fonts/" + filename, data)
}

#context {
  let pairs = query(<metadata-asset>).map(meta => meta.value)
  let seen = (:)
  for pair in pairs {
    let path = pair.at(0)
    while path.starts-with("../") {
      path = path.slice(3)
    }
    if path in seen { continue }
    seen.insert(path, true)
    asset(path, pair.at(1))
  }
}
