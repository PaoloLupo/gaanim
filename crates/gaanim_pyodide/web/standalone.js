// The recording as one HTML page with the embed player inside: it opens from
// a disk, or wherever the page is uploaded, and plays with no server, so
// neither CORS nor a place to publish the file stand in the way.
//
// The page is the embed player's (`reproductor/insertar/`), as the site
// serves it: its markup and styles, its script inline, the bindings imported
// from a `data:` address, and the module (gzip-compressed), the recording and
// the options in blocks the script reads (crates/gaanim_embed/web/main.js
// describes them). In base64 the module takes about 3.7 MB, and the
// recording a third more than its file.

// The page for `bundle`, played by the embed player at `embedUrl` with the
// options `loop`, `controls` and `autoplay`, as a Blob. `title` names it in
// the browser's tab and `generator` says what made it.
export async function standalonePage(embedUrl, bundle, { title, generator, ...options }) {
  const page = await text(new URL("index.html", embedUrl), { cache: "no-cache" });
  const scriptTag = page.match(/<script type="module" src="([^"]+)"><\/script>/);
  if (!scriptTag) throw new Error("la página del reproductor ligero no tiene la forma esperada");
  const script = await text(new URL(scriptTag[1], embedUrl));
  const glue = script.match(/from "(\.\/pkg\/[^"]+\.js(?:\?[^"]*)?)"/);
  const wasm = script.match(/"(pkg\/[^"]+\.wasm\.gz(?:\?[^"]*)?)"/);
  if (!glue || !wasm) throw new Error("el script del reproductor ligero no tiene la forma esperada");
  const [bindings, compressed] = await Promise.all([
    bytes(new URL(glue[1], embedUrl)),
    bytes(new URL(wasm[1], embedUrl)),
  ]);
  // A server may hand the module over already decoded.
  const module = compressed[0] === 0x1f && compressed[1] === 0x8b ? compressed : await gzip(compressed);

  const inline = script
    .replace(glue[1], () => `data:text/javascript;base64,${base64(bindings)}`)
    // Inline, these would end the script or open an HTML comment.
    .replace(/<\/(script)/gi, "<\\/$1")
    .replace(/<!--/g, "<\\!--");
  let html = page
    .replace(/<!--[\s\S]*?-->\s*/g, "")
    // The files they announce are inside.
    .replace(/\s*<link rel="(?:modulepreload|preload)"[^>]*>/g, "")
    .replace(/<title>[\s\S]*?<\/title>/, () => `<title>${escapeHtml(title)}</title>\n  <meta name="generator" content="${escapeHtml(generator)}">`);
  const icon = html.match(/<link rel="icon" href="([^"]+)">/);
  if (icon) {
    const data = await bytes(new URL(icon[1], embedUrl)).catch(() => null);
    html = html.replace(icon[0], () => (data ? `<link rel="icon" href="data:image/x-icon;base64,${base64(data)}">` : ""));
  }
  const [before, after] = html.split(scriptTag[0]);
  if (after === undefined) throw new Error("la página del reproductor ligero no tiene la forma esperada");
  return new Blob(
    [
      before,
      `<script type="application/json" id="gaanim-options">${JSON.stringify(booleans(options))}</script>\n`,
      `  <script type="application/octet-stream" id="gaanim-module">`,
      base64(module),
      `</script>\n  <script type="application/octet-stream" id="gaanim-file">`,
      base64(new Uint8Array(bundle)),
      `</script>\n  <script type="module">\n${inline}\n  </script>`,
      after,
    ],
    { type: "text/html" },
  );
}

async function text(url, init) {
  const response = await fetch(url, init);
  if (!response.ok) throw new Error(`${url.pathname}: HTTP ${response.status}`);
  return response.text();
}

async function bytes(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url.pathname}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

async function gzip(data) {
  const stream = new Blob([data]).stream().pipeThrough(new CompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

function base64(data) {
  if (data.toBase64) return data.toBase64();
  // In pieces: `String.fromCharCode` takes so many arguments at most.
  let binary = "";
  for (let i = 0; i < data.length; i += 0x8000) binary += String.fromCharCode(...data.subarray(i, i + 0x8000));
  return btoa(binary);
}

// Only booleans reach the page, so the block can hold nothing else.
function booleans(options) {
  return Object.fromEntries(Object.entries(options).filter(([, value]) => typeof value === "boolean"));
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[char]);
}
