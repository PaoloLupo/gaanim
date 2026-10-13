// A page per topic of the example catalog, on the site only: the book lists
// the topics in the examples index.
#import "../../components/section.typ": docs-chapter
#import "../../components/examples.typ": topic-page, topic-route, topics

#context if target() in ("bundle", "html") {
  for topic in topics {
    docs-chapter(
      route: topic-route(topic),
      title: topic.title,
      description: topic.description,
      nav: topic.title,
      code-langs: (),
      topic-page(topic),
    )
  }
}
