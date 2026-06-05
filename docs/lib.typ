#let docs(
  content-base: "/",
  asset-base: "assets/",
) = {
  assert(content-base.starts-with("/") and content-base.ends-with("/"))
  assert(asset-base.ends-with("/"))

  set stdx.config(
    content-base: content-base,
    asset-base: asset-base,
  )

  context if target() == "bundle" {
    include "assets/index.typ"
  }

  include "content/index.typ"
}
