// Gaanim documentation site behaviour.
//
// The script is loaded from `<site>/assets/script.js`; every other URL is
// resolved against it so the site works under any base path (GitHub Pages).
const ASSET_BASE = new URL(".", document.currentScript ? document.currentScript.src : window.location.href);
const SITE_ROOT = new URL("..", ASSET_BASE);

// ============================================================================
// Table of contents: highlight the section in view
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const tocLinks = document.querySelectorAll(".toc-sidebar a");
    if (tocLinks.length === 0) return;

    const observer = new IntersectionObserver((entries) => {
        entries.forEach((entry) => {
            if (!entry.isIntersecting) return;
            const id = entry.target.getAttribute("id");
            if (!id) return;
            tocLinks.forEach((link) => link.classList.remove("toc-active"));
            const active = document.querySelector(`.toc-sidebar a[href="#${CSS.escape(id)}"]`);
            if (active) active.classList.add("toc-active");
        });
    }, { rootMargin: "-70px 0px -75% 0px" });

    document.querySelectorAll("main h1, main h2, main h3, main h4").forEach((h) => observer.observe(h));
});

// ============================================================================
// Reading progress: the header's timeline playhead follows the scroll
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const root = document.documentElement;
    let queued = false;
    const update = () => {
        queued = false;
        const range = root.scrollHeight - window.innerHeight;
        const progress = range > 0 ? Math.min(1, Math.max(0, window.scrollY / range)) : 0;
        root.style.setProperty("--page-progress", progress.toFixed(4));
        root.style.setProperty("--page-progress-on", progress > 0 ? "1" : "0");
    };
    const schedule = () => {
        if (!queued) {
            queued = true;
            requestAnimationFrame(update);
        }
    };
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    update();
});

// ============================================================================
// Copy buttons on code blocks
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll(".code-source").forEach((sourceDiv) => {
        const pre = sourceDiv.querySelector("pre");
        if (!pre) return;

        const copyBtn = document.createElement("button");
        copyBtn.type = "button";
        copyBtn.className = "copy-code-btn";
        copyBtn.textContent = "Copiar";
        copyBtn.addEventListener("click", () => {
            navigator.clipboard.writeText(pre.innerText).then(() => {
                copyBtn.textContent = "¡Copiado!";
                copyBtn.classList.add("copied");
                setTimeout(() => {
                    copyBtn.textContent = "Copiar";
                    copyBtn.classList.remove("copied");
                }, 2000);
            }).catch((err) => console.error("Error al copiar el código: ", err));
        });
        sourceDiv.appendChild(copyBtn);
    });
});

// ============================================================================
// Heading permalinks
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll("main h1[id], main h2[id], main h3[id], main h4[id]").forEach((heading) => {
        const anchor = document.createElement("a");
        anchor.href = "#" + heading.id;
        anchor.className = "heading-anchor";
        anchor.textContent = "#";
        anchor.title = "Copiar enlace a esta sección";
        anchor.addEventListener("click", () => {
            const url = window.location.origin + window.location.pathname + anchor.hash;
            navigator.clipboard.writeText(url).catch((err) => console.error(err));
        });
        heading.appendChild(anchor);
    });
});

// ============================================================================
// Image lightbox
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const images = document.querySelectorAll(".code-result img, .code-result-only img, .anim-preview img");
    images.forEach((img) => {
        img.style.cursor = "zoom-in";
        img.addEventListener("click", () => {
            const modal = document.createElement("div");
            modal.className = "lightbox-modal";
            const closeBtn = document.createElement("span");
            closeBtn.className = "lightbox-close";
            closeBtn.innerHTML = "&times;";
            const modalImg = document.createElement("img");
            modalImg.src = img.src;
            modalImg.className = "lightbox-content";
            modal.append(closeBtn, modalImg);
            document.body.appendChild(modal);

            const closeModal = () => {
                modal.classList.add("lightbox-fade-out");
                document.removeEventListener("keydown", escListener);
                setTimeout(() => modal.remove(), 200);
            };
            const escListener = (e) => { if (e.key === "Escape") closeModal(); };
            closeBtn.addEventListener("click", closeModal);
            modal.addEventListener("click", (e) => { if (e.target === modal) closeModal(); });
            document.addEventListener("keydown", escListener);
            requestAnimationFrame(() => modal.classList.add("lightbox-show"));
        });
    });
});

// ============================================================================
// Mobile navigation drawer
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const toggleBtn = document.getElementById("nav-toggle-btn");
    const backdrop = document.getElementById("nav-backdrop");
    if (!toggleBtn) return;
    const close = () => document.body.classList.remove("nav-open");
    toggleBtn.addEventListener("click", () => document.body.classList.toggle("nav-open"));
    if (backdrop) backdrop.addEventListener("click", close);
    document.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });

    // Keep the active page visible in a long sidebar.
    const active = document.querySelector(".nav-sidebar a.nav-active");
    if (active) active.scrollIntoView({ block: "center" });
});

// ============================================================================
// Theme toggle (light / dark)
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const btn = document.getElementById("theme-toggle-btn");
    if (!btn) return;
    const root = document.documentElement;
    const systemTheme = () => window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    const storedTheme = () => { try { return localStorage.getItem("theme"); } catch (_e) { return null; } };
    const currentTheme = () => storedTheme() || systemTheme();
    const updateButton = () => {
        const dark = currentTheme() === "dark";
        btn.textContent = dark ? "☀" : "☾";
        btn.title = dark ? "Usar tema claro" : "Usar tema oscuro";
    };
    const applyTheme = (theme) => {
        if (theme) root.setAttribute("data-theme", theme);
        else root.removeAttribute("data-theme");
        updateButton();
    };
    applyTheme(storedTheme());
    btn.addEventListener("click", () => {
        const next = currentTheme() === "dark" ? "light" : "dark";
        try { localStorage.setItem("theme", next); } catch (_e) { /* private mode */ }
        applyTheme(next);
    });
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
        if (!storedTheme()) applyTheme(null);
    });
});

// ============================================================================
// Smart search: API symbols + documentation pages
// ============================================================================

const normalize = (value) => (value || "")
    .toLocaleLowerCase()
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "");

// Split identifiers and prose into comparable word tokens:
// `Drawable.grow_from_center` -> ["drawable", "grow", "from", "center"].
const words = (value) => normalize(value)
    .split(/[^a-z0-9]+/)
    .filter(Boolean);

const compact = (value) => normalize(value).replace(/[^a-z0-9]/g, "");

// Spanish (and a few English) task words mapped to API vocabulary, so
// "flecha" finds `arrow` and "mover" finds `move_to`.
const SYNONYMS = {
    flecha: ["arrow"], flechas: ["arrow"], circulo: ["circle"], circulos: ["circle"],
    rectangulo: ["rect"], cuadrado: ["square", "rect"], punto: ["dot", "point"],
    linea: ["line"], lineas: ["line"], curva: ["curve", "bezier"], arco: ["arc"],
    poligono: ["polygon"], estrella: ["star"], elipse: ["ellipse"], llave: ["brace"],
    texto: ["text"], titulo: ["title", "text"], ecuacion: ["equation", "math"],
    formula: ["equation", "math"], matriz: ["matrix"], tabla: ["table"], codigo: ["code"],
    imagen: ["image"], video: ["video", "media"], sonido: ["audio"], musica: ["audio"],
    mover: ["move", "shift"], mueve: ["move", "shift"], desplazar: ["shift", "move"],
    posicion: ["move", "position"], rotar: ["rotate"], girar: ["rotate", "spin"],
    escalar: ["scale"], tamano: ["scale", "size"], agrandar: ["scale", "grow"],
    crecer: ["grow"], crece: ["grow"], encoger: ["shrink"], aparecer: ["fade", "create", "write"],
    aparece: ["fade", "create"], desaparecer: ["fade", "uncreate"], ocultar: ["fade", "hide"],
    escribir: ["write"], dibujar: ["create", "draw"], trazar: ["create", "draw"],
    borrar: ["uncreate", "unwrite", "remove"], transformar: ["transform", "morph"],
    resaltar: ["indicate", "highlight", "flash"], destacar: ["indicate", "circumscribe"],
    parpadear: ["flash"], vibrar: ["wiggle"], seguir: ["follow", "along"], camino: ["path"],
    ruta: ["path"], color: ["fill", "color"], rellenar: ["fill"], relleno: ["fill"],
    borde: ["stroke"], contorno: ["stroke"], grosor: ["width", "stroke"], opacidad: ["opacity"],
    transparencia: ["opacity"], sombra: ["shadow"], brillo: ["glow"], desenfoque: ["blur"],
    degradado: ["gradient"], tema: ["theme"], estilo: ["style"], fuente: ["font"],
    camara: ["camera"], acercar: ["zoom"], alejar: ["zoom"], encuadrar: ["frame"],
    esperar: ["wait"], pausa: ["wait", "stop"], pausar: ["stop", "wait"], reproducir: ["play"],
    duracion: ["duration"], velocidad: ["speed", "duration"], retraso: ["delay"],
    suavizado: ["easing"], suave: ["smooth", "easing"], rebote: ["spring", "bounce"],
    exportar: ["export", "render"], renderizar: ["render"], guardar: ["export", "save"],
    grafica: ["chart", "plot", "axes"], grafico: ["chart", "plot"], ejes: ["axes", "axis"],
    eje: ["axis", "axes"], funcion: ["function", "plot"], datos: ["data", "chart"],
    barras: ["bar", "chart"], campo: ["field", "vector"],
    grupo: ["group"], agrupar: ["group"], alinear: ["align", "anchor"], ordenar: ["arrange", "layout"],
    diapositiva: ["slide", "segment"], presentacion: ["slide", "segment", "stop"],
    seccion: ["segment", "section"], segmento: ["segment"], actualizar: ["updater"],
    reactivo: ["signal", "updater", "parameter"], parametro: ["parameter"],
    fondo: ["background"], escena: ["scene"], lienzo: ["canvas", "scene"],
};

const KIND_LABELS = {
    class: "clase", constructor: "constructor", method: "método", property: "propiedad",
    attribute: "atributo", function: "función", constant: "constante", type: "tipo",
    factory: "fábrica",
};

// Pages for symbols without a hand-written reference entry.
const CLASS_PAGES = [
    [/^(Scene|Segment|SceneStop|Transition|Composition|Schedule|ScheduleEntry|Camera\w*)$/, "referencia/scene/"],
    [/^(Anim|Easing|EasingCurve|Updater|TextSelectionAnimation)$/, "referencia/animations/"],
    [/^(Text\w*|Typography|Parts?)$/, "referencia/text/"],
    [/^(Layout\w*|Anchor|AnchorPoint|TextAnchor|Direction|ConstraintSet|PointRef|Align)$/, "referencia/layout/"],
    [/^(Color|ColorMap|Brush|Background|StrokeStyle|Style|Theme|AxesStyle|Glow|Shadow)$/, "referencia/themes/"],
    [/^(Axis|Scale|Field|Value|Guide|ChartSpec|Chart\w*|Visualization|Data\w*|Computed|Parameter|Variable|TimeInput|Matrix\w*|Plot\w*|Axes\w*|Number\w*)$/, "referencia/visualization/"],
    [/^(Audio\w*)$/, "referencia/audio/"],
    [/^(Asset\w*|Media\w*|Video\w*|Image\w*|Svg\w*|Lottie\w*)$/, "referencia/assets/"],
];
const pageForClass = (cls) => {
    if (!cls) return "referencia/";
    const match = CLASS_PAGES.find(([re]) => re.test(cls));
    return match ? match[1] : "referencia/objetos/";
};
const PAGE_LABELS = {
    "referencia/": "API", "referencia/scene/": "Escena", "referencia/animations/": "Animaciones", "referencia/text/": "Texto",
    "referencia/layout/": "Layout", "referencia/themes/": "Temas", "referencia/visualization/": "Visualización",
    "referencia/audio/": "Audio", "referencia/assets/": "Recursos", "referencia/objetos/": "Objetos",
};

// The `.animate` proxy returns `Anim`, but the reference documents those
// effects under `Drawable`; treat them as the same owner when linking.
const sameOwner = (a, b) => a === b || (["Anim", "Drawable"].includes(a) && ["Anim", "Drawable"].includes(b));

let apiIndexPromise = null;
const loadApiIndex = () => {
    if (apiIndexPromise) return apiIndexPromise;
    apiIndexPromise = fetch(new URL("api-index.json", ASSET_BASE))
        .then((response) => (response.ok ? response.json() : {}))
        .catch(() => ({}))
        .then(buildApiEntries);
    return apiIndexPromise;
};

function buildApiEntries(index) {
    // Expand "Drawable.fade_in / fade_out / fade_to" into owner + members.
    const documented = (index.documented || []).map((entry) => {
        const parts = entry.name.split(/\s*[\/,]\s*/).map((p) => p.trim().split(/\s+/)[0]).filter(Boolean);
        const first = (parts[0] || "").replace(".animate.", ".");
        const owner = first.includes(".") ? first.slice(0, first.lastIndexOf(".")) : null;
        const members = parts.map((p) => {
            const clean = p.replace(".animate.", ".");
            return clean.includes(".") ? clean.slice(clean.lastIndexOf(".") + 1) : clean;
        });
        const url = new URL(entry.route.replace(/^\//, "") + "#" + entry.anchor, SITE_ROOT).href;
        return { ...entry, owner, members, url };
    });

    const entries = [];
    const linked = new Set();
    for (const symbol of index.symbols || []) {
        const member = symbol.name.includes(".") ? symbol.name.slice(symbol.name.lastIndexOf(".") + 1) : symbol.name;
        const candidates = documented.filter((d) => d.members.includes(member));
        const doc = candidates.find((d) => sameOwner(d.owner, symbol.class))
            || (symbol.kind === "class" ? null : candidates.find((d) => !d.owner));
        if (doc) linked.add(doc);
        const fallbackPage = pageForClass(symbol.class || (symbol.kind === "class" ? symbol.name : null));
        entries.push({
            name: symbol.name,
            kind: symbol.kind,
            signature: symbol.signature,
            summary: symbol.summary || (doc ? doc.summary : ""),
            where: doc ? "Referencia" : PAGE_LABELS[fallbackPage] || "API",
            url: doc ? doc.url : new URL(fallbackPage, SITE_ROOT).href,
            documented: Boolean(doc),
        });
    }
    // Reference entries that describe concepts rather than one stub symbol
    // (e.g. "Anim timing", "parallel / sequence / stagger").
    for (const doc of documented) {
        if (linked.has(doc)) continue;
        entries.push({
            name: doc.name,
            kind: doc.kind,
            signature: doc.signature,
            summary: doc.summary,
            where: "Referencia",
            url: doc.url,
            documented: true,
        });
    }
    return entries.map((entry) => ({
        ...entry,
        nameCompact: compact(entry.name),
        memberCompact: compact(entry.name.slice(entry.name.lastIndexOf(".") + 1)),
        nameWords: words(entry.name),
        summaryWords: new Set(words(`${entry.summary} ${entry.signature}`)),
    }));
}

// Characters of `needle` appear in order in `hay`; returns a 0..1 closeness.
function subsequenceScore(needle, hay) {
    if (!needle) return 0;
    let pos = -1;
    let gaps = 0;
    for (const ch of needle) {
        const next = hay.indexOf(ch, pos + 1);
        if (next < 0) return 0;
        if (pos >= 0) gaps += next - pos - 1;
        pos = next;
    }
    return needle.length / (needle.length + gaps);
}

function expandQuery(query) {
    const tokens = words(query);
    const expanded = [];
    tokens.forEach((token) => {
        expanded.push({ token, weight: 1, of: token });
        (SYNONYMS[token] || []).forEach((syn) => {
            words(syn).forEach((w) => expanded.push({ token: w, weight: 0.8, of: token }));
        });
    });
    return { tokens, expanded };
}

function scoreApi(entry, query, { tokens, expanded }) {
    const q = compact(query);
    if (!q) return 0;
    let score = 0;

    // Exact and prefix matches on the member or the qualified name.
    if (entry.memberCompact === q || entry.nameCompact === q) score += 120;
    else if (entry.memberCompact.startsWith(q)) score += 80;
    else if (entry.nameCompact.startsWith(q)) score += 70;
    else if (q.length >= 3 && entry.nameCompact.includes(q)) score += 50;

    // Every original word should be found in the name, via a synonym, or in
    // the summary.
    let matched = 0;
    for (const token of tokens) {
        let best = 0;
        for (const { token: t, weight, of } of expanded) {
            if (of !== token) continue;
            if (entry.nameWords.includes(t)) best = Math.max(best, 30 * weight);
            else if (t.length >= 2 && entry.nameWords.some((w) => w.startsWith(t))) best = Math.max(best, 22 * weight);
            else if (entry.summaryWords.has(t)) best = Math.max(best, 8 * weight);
        }
        if (best === 0 && token.length >= 3) {
            // Per-word typo tolerance: "ply" still finds `play`.
            for (const w of entry.nameWords) {
                if (w[0] !== token[0]) continue;
                const fuzzy = subsequenceScore(token, w) * Math.min(1, token.length / w.length + 0.25);
                if (fuzzy >= 0.6) best = Math.max(best, 18 * fuzzy);
            }
        }
        if (best > 0) matched += 1;
        score += best;
    }
    if (tokens.length > 1 && matched < tokens.length) score *= matched / tokens.length / 1.5;
    if (matched === 0 && score < 50) {
        // Fuzzy fallback for typos and abbreviations such as "grwfrm".
        const fuzzy = subsequenceScore(q, entry.memberCompact);
        if (q.length >= 3 && fuzzy > 0.5) score += 40 * fuzzy;
    }

    if (score <= 0) return 0;
    if (entry.documented) score += 6;
    if (entry.kind === "class" || entry.kind === "constructor") score += 4;
    if (entry.kind === "attribute" || entry.kind === "constant") score -= 4;
    return score - entry.name.length * 0.05;
}

// Documentation pages, loaded lazily from the sidebar links.
let pageIndexPromise = null;
const canonicalUrl = (value) => {
    const url = new URL(value, window.location.href);
    url.hash = "";
    if (url.pathname.endsWith("/index.html")) url.pathname = url.pathname.slice(0, -"index.html".length);
    return url.href;
};
const pageFromDocument = (doc, url, fallbackTitle) => {
    const main = doc.querySelector("main") || doc.body;
    const title = main.querySelector("h1")?.textContent.replace(/#$/, "").trim()
        || doc.title.replace(/\s+—\s+Gaanim\s*$/, "").trim()
        || fallbackTitle;
    const description = doc.querySelector('meta[name="description"]')?.content || "";
    const headings = Array.from(main.querySelectorAll("h2, h3, h4"))
        .map((h) => ({ id: h.id, text: h.textContent.replace(/#$/, "").trim() }))
        .filter((h) => h.text.length > 0);
    const text = main.textContent.replace(/\s+/g, " ").trim();
    return { url, title, description, headings, text, normalizedText: normalize(`${title} ${description} ${text}`) };
};
const loadPageIndex = () => {
    if (pageIndexPromise) return pageIndexPromise;
    const nav = document.getElementById("global-nav-sidebar");
    const seen = new Set();
    const pages = nav ? Array.from(nav.querySelectorAll("a[href]"))
        .map((link) => ({ url: canonicalUrl(link.href), label: link.textContent.trim() }))
        .filter((p) => (seen.has(p.url) ? false : (seen.add(p.url), true))) : [];
    pageIndexPromise = Promise.all(pages.map(async (page) => {
        if (canonicalUrl(window.location.href) === page.url) return pageFromDocument(document, page.url, page.label);
        try {
            const response = await fetch(page.url, { credentials: "same-origin" });
            if (!response.ok) return null;
            const doc = new DOMParser().parseFromString(await response.text(), "text/html");
            return pageFromDocument(doc, page.url, page.label);
        } catch (_e) {
            return null;
        }
    })).then((list) => list.filter(Boolean));
    return pageIndexPromise;
};

function rankPages(pages, query, { tokens, expanded }) {
    if (tokens.length === 0) return [];
    return pages.map((page) => {
        const title = normalize(page.title);
        let score = 0;
        let matched = 0;
        for (const token of tokens) {
            let best = 0;
            for (const { token: t, weight, of } of expanded) {
                if (of !== token) continue;
                if (title.includes(t)) best = Math.max(best, 12 * weight);
                else if (page.headings.some((h) => normalize(h.text).includes(t))) best = Math.max(best, 6 * weight);
                else if (page.normalizedText.includes(t)) best = Math.max(best, 1.5 * weight);
            }
            if (best > 0) matched += 1;
            score += best;
        }
        if (matched < tokens.length) return null;
        const phrase = normalize(query).trim();
        if (phrase.length > 3 && page.normalizedText.includes(phrase)) score += 5;
        const section = page.headings.find((h) => tokens.some((t) => normalize(h.text).includes(t)));
        return { page, score, section, href: page.url + (section?.id ? `#${section.id}` : "") };
    }).filter(Boolean).sort((a, b) => b.score - a.score).slice(0, 6);
}

function snippetFor(text, query) {
    const token = words(query)[0];
    const at = token ? normalize(text).indexOf(token) : -1;
    if (at < 0) return text.slice(0, 150);
    const start = Math.max(0, at - 50);
    return `${start > 0 ? "…" : ""}${text.slice(start, start + 160)}…`;
}

// Wrap the characters of `text` that match the query words in <mark>.
function highlight(text, query) {
    const tokens = words(query).filter((t) => t.length > 1);
    const fragment = document.createDocumentFragment();
    const norm = normalize(text);
    if (tokens.length === 0 || norm.length !== text.length) {
        fragment.append(text);
        return fragment;
    }
    const marks = new Array(text.length).fill(false);
    tokens.forEach((t) => {
        let i = norm.indexOf(t);
        while (i >= 0) {
            for (let k = i; k < i + t.length; k++) marks[k] = true;
            i = norm.indexOf(t, i + t.length);
        }
    });
    let buffer = "";
    let marking = false;
    const flush = () => {
        if (!buffer) return;
        if (marking) {
            const m = document.createElement("mark");
            m.textContent = buffer;
            fragment.append(m);
        } else {
            fragment.append(buffer);
        }
        buffer = "";
    };
    for (let i = 0; i < text.length; i++) {
        if (marks[i] !== marking) {
            flush();
            marking = marks[i];
        }
        buffer += text[i];
    }
    flush();
    return fragment;
}

document.addEventListener("DOMContentLoaded", () => {
    const FILTERS = [
        ["all", "Todo"], ["api", "API"], ["method", "Métodos"], ["class", "Clases"], ["pages", "Guías"],
    ];
    const TIPS = ["mover un objeto", "flecha", "Scene.play", "cámara zoom", "exportar video", "ecuación", "easing"];

    const overlay = document.createElement("div");
    overlay.className = "search-overlay";
    overlay.hidden = true;
    overlay.innerHTML = `
        <div class="search-panel" role="dialog" aria-modal="true" aria-label="Buscar en la documentación">
            <div class="search-input-row">
                <span class="search-icon" aria-hidden="true">⌕</span>
                <input class="search-input" type="search" autocomplete="off" spellcheck="false"
                    placeholder="Busca un método, una clase o una tarea: «mover», «flecha», «Scene.play»…"
                    role="combobox" aria-expanded="true" aria-controls="search-results" />
                <kbd>Esc</kbd>
            </div>
            <div class="search-filters" role="toolbar" aria-label="Filtrar resultados"></div>
            <div class="search-results" id="search-results" role="listbox"></div>
            <div class="search-footer">
                <span><kbd>↑</kbd><kbd>↓</kbd> navegar</span>
                <span><kbd>Enter</kbd> abrir</span>
                <span><kbd>Tab</kbd> cambiar filtro</span>
                <span><kbd>Ctrl</kbd><kbd>K</kbd> buscar desde cualquier página</span>
            </div>
        </div>`;
    document.body.appendChild(overlay);

    const input = overlay.querySelector(".search-input");
    const resultsBox = overlay.querySelector(".search-results");
    const filtersBox = overlay.querySelector(".search-filters");
    let filter = "all";
    let items = [];
    let selected = 0;
    let version = 0;
    let lastFocus = null;

    const setFilter = (id) => {
        filter = id;
        filtersBox.querySelectorAll(".search-filter").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.filter === id)));
        run();
        input.focus();
    };
    FILTERS.forEach(([id, label]) => {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "search-filter";
        btn.dataset.filter = id;
        btn.textContent = label;
        btn.setAttribute("aria-pressed", String(id === filter));
        btn.addEventListener("click", () => setFilter(id));
        filtersBox.appendChild(btn);
    });

    const open = (initial = "") => {
        lastFocus = document.activeElement;
        overlay.hidden = false;
        document.body.style.overflow = "hidden";
        input.value = initial;
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
        loadApiIndex();
        loadPageIndex();
        run();
    };
    const close = () => {
        overlay.hidden = true;
        document.body.style.overflow = "";
        if (lastFocus && lastFocus.focus && lastFocus.id !== "home-search-input") lastFocus.focus();
    };

    const select = (index) => {
        if (items.length === 0) return;
        selected = (index + items.length) % items.length;
        items.forEach((node, i) => node.setAttribute("aria-selected", String(i === selected)));
        items[selected].scrollIntoView({ block: "nearest" });
    };

    const renderEmpty = (message, withTips) => {
        resultsBox.replaceChildren();
        const box = document.createElement("div");
        box.className = "search-empty";
        box.innerHTML = message;
        if (withTips) {
            const tips = document.createElement("div");
            tips.className = "search-tips";
            TIPS.forEach((tip) => {
                const b = document.createElement("button");
                b.type = "button";
                b.className = "search-tip";
                b.textContent = tip;
                b.addEventListener("click", () => {
                    input.value = tip;
                    run();
                    input.focus();
                });
                tips.appendChild(b);
            });
            box.appendChild(tips);
        }
        resultsBox.appendChild(box);
        items = [];
    };

    const apiNode = (entry, query) => {
        const a = document.createElement("a");
        a.className = "search-result";
        a.href = entry.url;
        a.setAttribute("role", "option");
        const badge = document.createElement("span");
        badge.className = `kind-badge kind-${entry.kind}`;
        badge.textContent = KIND_LABELS[entry.kind] || entry.kind;
        const name = document.createElement("span");
        name.className = "search-result-name";
        name.append(highlight(entry.name, query));
        const where = document.createElement("span");
        where.className = "search-result-where";
        where.textContent = entry.where;
        a.append(badge, name, where);
        if (entry.signature && entry.kind !== "class") {
            const sig = document.createElement("span");
            sig.className = "search-result-signature";
            sig.textContent = entry.signature;
            a.append(sig);
        }
        if (entry.summary) {
            const summary = document.createElement("span");
            summary.className = "search-result-summary";
            summary.textContent = entry.summary;
            a.append(summary);
        }
        return a;
    };

    const pageNode = (result, query) => {
        const a = document.createElement("a");
        a.className = "search-result search-result-page";
        a.href = result.href;
        a.setAttribute("role", "option");
        const icon = document.createElement("span");
        icon.className = "search-result-icon";
        icon.textContent = "¶";
        const title = document.createElement("span");
        title.className = "search-result-title";
        title.append(highlight(result.page.title, query));
        a.append(icon, title);
        if (result.section) {
            const section = document.createElement("span");
            section.className = "search-result-section";
            section.textContent = `§ ${result.section.text}`;
            a.append(section);
        }
        const summary = document.createElement("span");
        summary.className = "search-result-summary";
        summary.textContent = snippetFor(result.page.text || result.page.description, query);
        a.append(summary);
        return a;
    };

    const group = (title) => {
        const div = document.createElement("div");
        div.className = "search-group-title";
        div.textContent = title;
        return div;
    };

    async function run() {
        const query = input.value.trim();
        const current = ++version;
        if (!query) {
            renderEmpty("Busca por nombre (<strong>move_to</strong>, <strong>Scene.play</strong>) o describe lo que quieres hacer.", true);
            return;
        }
        const parsed = expandQuery(query);
        const wantApi = filter !== "pages";
        const wantPages = filter === "all" || filter === "pages";
        const [api, pages] = await Promise.all([
            wantApi ? loadApiIndex() : Promise.resolve([]),
            wantPages ? loadPageIndex() : Promise.resolve([]),
        ]);
        if (current !== version) return;

        const seen = new Set();
        const apiResults = api
            .filter((entry) => filter !== "method" || ["method", "property", "function"].includes(entry.kind))
            .filter((entry) => filter !== "class" || ["class", "constructor"].includes(entry.kind))
            .map((entry) => ({ entry, score: scoreApi(entry, query, parsed) }))
            .filter((r) => r.score > 0)
            .sort((a, b) => b.score - a.score)
            // Collapse duplicates that point at the same place with the same name.
            .filter(({ entry }) => {
                const key = `${entry.name}|${entry.url}`;
                return seen.has(key) ? false : (seen.add(key), true);
            })
            .slice(0, filter === "all" ? 8 : 30);
        const pageResults = wantPages ? rankPages(pages, query, parsed) : [];

        resultsBox.replaceChildren();
        if (apiResults.length === 0 && pageResults.length === 0) {
            renderEmpty(`Sin resultados para <strong>${query.replace(/[<>&"]/g, "")}</strong>. Prueba con otra palabra:`, true);
            return;
        }
        if (apiResults.length > 0) {
            resultsBox.appendChild(group("API de Python"));
            apiResults.forEach(({ entry }) => resultsBox.appendChild(apiNode(entry, query)));
        }
        if (pageResults.length > 0) {
            resultsBox.appendChild(group("Guías y páginas"));
            pageResults.forEach((r) => resultsBox.appendChild(pageNode(r, query)));
        }
        items = Array.from(resultsBox.querySelectorAll(".search-result"));
        items.forEach((node, i) => node.addEventListener("mousemove", () => { if (selected !== i) select(i); }));
        select(0);
    }

    input.addEventListener("input", run);
    input.addEventListener("keydown", (e) => {
        if (e.key === "ArrowDown") { e.preventDefault(); select(selected + 1); }
        else if (e.key === "ArrowUp") { e.preventDefault(); select(selected - 1); }
        else if (e.key === "Enter") {
            const node = items[selected];
            if (node) { e.preventDefault(); close(); window.location.href = node.href; }
        } else if (e.key === "Tab") {
            e.preventDefault();
            const ids = FILTERS.map(([id]) => id);
            setFilter(ids[(ids.indexOf(filter) + (e.shiftKey ? -1 : 1) + ids.length) % ids.length]);
        } else if (e.key === "Escape") { e.preventDefault(); close(); }
    });
    overlay.addEventListener("click", (e) => { if (e.target === overlay) close(); });
    resultsBox.addEventListener("click", (e) => { if (e.target.closest(".search-result")) close(); });

    document.addEventListener("keydown", (e) => {
        const target = e.target;
        const typing = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target.isContentEditable;
        if ((e.key === "k" || e.key === "K") && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            if (overlay.hidden) open(); else close();
        } else if (e.key === "/" && !typing && overlay.hidden) {
            e.preventDefault();
            open();
        }
    });

    const trigger = document.getElementById("search-trigger");
    if (trigger) {
        trigger.addEventListener("click", () => open());
        const kbd = trigger.querySelector(".search-trigger-kbd");
        if (kbd && /Mac|iPhone|iPad/.test(navigator.platform)) kbd.textContent = "⌘ K";
    }
    const homeInput = document.getElementById("home-search-input");
    if (homeInput) {
        // The hero box is a large entry point into the same palette.
        homeInput.addEventListener("focus", () => {
            const value = homeInput.value;
            homeInput.value = "";
            homeInput.blur();
            open(value);
        });
    }

    // Deep links to API entries get a brief highlight.
    const flashTarget = () => {
        const id = decodeURIComponent(window.location.hash.slice(1));
        const el = id && document.getElementById(id);
        if (el && el.classList.contains("api-entry")) {
            el.classList.add("api-flash");
            setTimeout(() => el.classList.remove("api-flash"), 1800);
        }
    };
    window.addEventListener("hashchange", flashTarget);
    flashTarget();
});
