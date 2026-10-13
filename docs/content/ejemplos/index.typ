#import "../../components/section.typ": docs-chapter
#import "../../components/examples.typ": examples-index

#show: docs-chapter.with(
  title: "Examples",
  description: "Every example in the repository, by topic: complete scenes ready to run",
  route: "/ejemplos/",
  nav: "Examples",
)

Each example covers one subject: it shows the variants the API offers for it,
with the code that builds each one on screen.

#examples-index()

The earlier gallery scenes, in Spanish, are still in
#link("/ejemplos/basicos/")[Ejemplos básicos] and
#link("/ejemplos/avanzados/")[Ejemplos avanzados].
