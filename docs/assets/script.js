document.addEventListener("DOMContentLoaded", () => {
    // Todos los enlaces del TOC
    const tocLinks = document.querySelectorAll('.toc-sidebar a');
    
    // Si no hay TOC en esta página, abortar
    if (tocLinks.length === 0) return;

    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                const id = entry.target.getAttribute('id');
                if (!id) return;

                // Remover clase activa de todos los enlaces
                tocLinks.forEach(link => {
                    link.classList.remove('toc-active');
                });

                // Añadir clase activa al enlace cuyo href coincida con el ID
                const activeLink = document.querySelector(`.toc-sidebar a[href="#${id}"]`);
                if (activeLink) {
                    activeLink.classList.add('toc-active');
                }
            }
        });
    }, { 
        // Observa la porción superior de la pantalla
        rootMargin: "0px 0px -75% 0px" 
    });

    // Observar todos los títulos
    document.querySelectorAll('h1, h2, h3, h4').forEach(h => {
        observer.observe(h);
    });
});

// ============================================================================
// Botón de Copiar Código
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll('.code-source').forEach(sourceDiv => {
        // Seleccionamos el bloque pre que contiene el texto
        const pre = sourceDiv.querySelector('pre');
        if (!pre) return;

        // Aseguramos que el contenedor padre tenga position relative
        sourceDiv.style.position = 'relative';

        // Creamos el botón
        const copyBtn = document.createElement('button');
        copyBtn.className = 'copy-code-btn';
        copyBtn.innerText = 'Copiar';

        // Lógica de copiado
        copyBtn.addEventListener('click', () => {
            navigator.clipboard.writeText(pre.innerText).then(() => {
                copyBtn.innerText = '¡Copiado!';
                copyBtn.classList.add('copied');
                setTimeout(() => {
                    copyBtn.innerText = 'Copiar';
                    copyBtn.classList.remove('copied');
                }, 2000);
            }).catch(err => {
                console.error('Error al copiar el código: ', err);
            });
        });

        sourceDiv.appendChild(copyBtn);
    });
});

// ============================================================================
// Enlaces Ancla (Permalinks) para Títulos
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll('h1[id], h2[id], h3[id], h4[id]').forEach(heading => {
        // Creamos el enlace ancla
        const anchor = document.createElement('a');
        anchor.href = '#' + heading.id;
        anchor.className = 'heading-anchor';
        anchor.innerText = '#';
        anchor.title = 'Copiar enlace a esta sección';
        
        // Al hacer clic, copiamos la URL completa al portapapeles
        anchor.addEventListener('click', (e) => {
            const url = window.location.origin + window.location.pathname + anchor.hash;
            navigator.clipboard.writeText(url).catch(err => console.error(err));
        });

        // Lo insertamos dentro del título (al final o al inicio)
        heading.appendChild(anchor);
    });
});

// ============================================================================
// Modal de Imágenes (Lightbox)
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    // Seleccionar todas las imágenes (tanto resultados mixtos como de "solo resultados")
    const images = document.querySelectorAll('.code-result img, .code-result-only img, .anim-preview img');
    if (images.length === 0) return;

    images.forEach(img => {
        // Indicar visualmente que la imagen es interactiva
        img.style.cursor = 'zoom-in';

        img.addEventListener('click', () => {
            // Crear el modal
            const modal = document.createElement('div');
            modal.className = 'lightbox-modal';

            // Crear el botón de cerrar
            const closeBtn = document.createElement('span');
            closeBtn.className = 'lightbox-close';
            closeBtn.innerHTML = '&times;';

            // Crear la imagen clonada
            const modalImg = document.createElement('img');
            modalImg.src = img.src;
            modalImg.className = 'lightbox-content';

            // Ensamblar el modal
            modal.appendChild(closeBtn);
            modal.appendChild(modalImg);
            document.body.appendChild(modal);

            // Funciones de cierre
            const closeModal = () => {
                modal.classList.add('lightbox-fade-out');
                setTimeout(() => {
                    if (document.body.contains(modal)) {
                        document.body.removeChild(modal);
                    }
                }, 300); // duración de la transición
            };

            // Cerrar al hacer clic en el botón de la X
            closeBtn.addEventListener('click', closeModal);

            // Cerrar al hacer clic fuera de la imagen (en el fondo oscuro)
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    closeModal();
                }
            });

            // Cerrar al presionar la tecla Escape
            const escListener = (e) => {
                if (e.key === 'Escape') {
                    closeModal();
                    document.removeEventListener('keydown', escListener);
                }
            };
            document.addEventListener('keydown', escListener);
            
            // Forzar reflujo para activar transición css
            requestAnimationFrame(() => {
                modal.classList.add('lightbox-show');
            });
        });
    });
});

// ============================================================================
// Toggle Sidebar Global (Menú Hamburguesa)
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const toggleBtn = document.getElementById("nav-toggle-btn");
    const sidebar = document.getElementById("global-nav-sidebar");
    
    if (toggleBtn && sidebar) {
        // Restaurar estado guardado
        const isCollapsed = localStorage.getItem("sidebar-collapsed");
        if (isCollapsed === "true") {
            sidebar.classList.add("collapsed");
        }

        toggleBtn.addEventListener("click", () => {
            sidebar.classList.toggle("collapsed");
            // Guardar estado
            localStorage.setItem("sidebar-collapsed", sidebar.classList.contains("collapsed"));
        });
    }
});

// ============================================================================
// Theme Toggle (Dark/Light Mode)
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const btn = document.getElementById("theme-toggle-btn");
    if (!btn) return;

    const root = document.documentElement;

    function getSystemTheme() {
        return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }

    function getStoredTheme() {
        return localStorage.getItem("theme");
    }

    function getCurrentTheme() {
        return getStoredTheme() || getSystemTheme();
    }

    function updateButton() {
        const theme = getCurrentTheme();
        btn.textContent = theme === "dark" ? "\u2600" : "\u263E";
    }

    function applyTheme(theme) {
        if (theme) {
            root.setAttribute("data-theme", theme);
        } else {
            root.removeAttribute("data-theme");
        }
        updateButton();
    }

    // Initialize
    applyTheme(getStoredTheme());

    // Toggle
    btn.addEventListener("click", () => {
        const current = getCurrentTheme();
        const next = current === "dark" ? "light" : "dark";
        localStorage.setItem("theme", next);
        applyTheme(next);
    });

    // Listen for system theme changes
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
        if (!getStoredTheme()) {
            applyTheme(null);
        }
    });
});

// ============================================================================
// Búsqueda global de documentación
// ============================================================================
document.addEventListener("DOMContentLoaded", () => {
    const input = document.getElementById("docs-search-input");
    const resultsBox = document.getElementById("docs-search-results");
    const nav = document.getElementById("global-nav-sidebar");
    if (!input || !resultsBox || !nav) return;

    let indexPromise = null;
    let searchVersion = 0;

    const normalize = (value) => value
        .toLocaleLowerCase()
        .normalize("NFD")
        .replace(/[\u0300-\u036f]/g, "");

    const canonicalUrl = (value) => {
        const url = new URL(value, window.location.href);
        url.hash = "";
        if (url.pathname.endsWith("/index.html")) {
            url.pathname = url.pathname.slice(0, -"index.html".length);
        }
        return url.href;
    };

    const pageLinks = () => {
        const seen = new Set();
        return Array.from(nav.querySelectorAll("a[href]"))
            .map((link) => {
                const url = canonicalUrl(link.href);
                return { url, label: link.textContent.trim() };
            })
            .filter((page) => {
                if (seen.has(page.url)) return false;
                seen.add(page.url);
                return true;
            });
    };

    const pageFromDocument = (doc, url, fallbackTitle) => {
        const main = doc.querySelector("main") || doc.body;
        const title = main.querySelector("h1")?.textContent.trim()
            || doc.title.replace(/\s+—\s+Gaanim\s*$/, "").trim()
            || fallbackTitle;
        const description = doc.querySelector('meta[name="description"]')?.content || "";
        const headings = Array.from(main.querySelectorAll("h2, h3, h4"))
            .map((heading) => ({
                id: heading.id,
                text: heading.textContent.trim(),
            }))
            .filter((heading) => heading.text.length > 0);
        const text = main.textContent.replace(/\s+/g, " ").trim();

        return {
            url,
            title,
            description,
            headings,
            text,
            normalizedText: normalize(`${title} ${description} ${text}`),
        };
    };

    const loadPage = async (page) => {
        if (canonicalUrl(window.location.href) === page.url) {
            return pageFromDocument(document, page.url, page.label);
        }

        try {
            const response = await fetch(page.url, { credentials: "same-origin" });
            if (!response.ok) return null;
            const html = await response.text();
            const doc = new DOMParser().parseFromString(html, "text/html");
            return pageFromDocument(doc, page.url, page.label);
        } catch (_error) {
            // A local file opened directly may block fetch; current-page search
            // still works and the failed page is simply omitted.
            return null;
        }
    };

    const buildIndex = () => Promise.all(pageLinks().map(loadPage));

    const snippetFor = (page, query) => {
        const source = page.text || page.description;
        const sourceNormalized = normalize(source);
        const firstToken = normalize(query).split(/\s+/).find(Boolean);
        const matchAt = firstToken ? sourceNormalized.indexOf(firstToken) : -1;
        if (matchAt < 0) return source.slice(0, 170);

        const start = Math.max(0, matchAt - 58);
        const end = Math.min(source.length, start + 178);
        return `${start > 0 ? "…" : ""}${source.slice(start, end)}${end < source.length ? "…" : ""}`;
    };

    const rankedResults = (pages, query) => {
        const normalizedQuery = normalize(query).trim();
        const tokens = normalizedQuery.split(/\s+/).filter(Boolean);
        if (tokens.length === 0) return [];

        return pages
            .filter(Boolean)
            .map((page) => {
                const title = normalize(page.title);
                const headings = normalize(page.headings.map((heading) => heading.text).join(" "));
                const matchesEveryToken = tokens.every((token) => page.normalizedText.includes(token));
                if (!matchesEveryToken) return null;

                let score = normalizedQuery.length > 2 && page.normalizedText.includes(normalizedQuery) ? 4 : 0;
                tokens.forEach((token) => {
                    if (title.includes(token)) score += 10;
                    if (headings.includes(token)) score += 5;
                    if (page.normalizedText.includes(token)) score += 1;
                });

                const section = page.headings.find((heading) => {
                    const headingText = normalize(heading.text);
                    return tokens.some((token) => headingText.includes(token));
                });
                const target = section?.id ? `#${section.id}` : "";

                return {
                    page,
                    section,
                    score,
                    href: `${page.url}${target}`,
                    snippet: snippetFor(page, query),
                };
            })
            .filter(Boolean)
            .sort((a, b) => b.score - a.score || a.page.title.localeCompare(b.page.title))
            .slice(0, 8);
    };

    const showMessage = (message) => {
        resultsBox.replaceChildren();
        const element = document.createElement("span");
        element.className = "docs-search-message";
        element.textContent = message;
        resultsBox.appendChild(element);
    };

    const renderResults = (results) => {
        resultsBox.replaceChildren();
        if (results.length === 0) {
            showMessage("No se encontraron resultados.");
            return;
        }

        results.forEach((result) => {
            const link = document.createElement("a");
            link.className = "docs-search-result";
            link.href = result.href;

            const title = document.createElement("span");
            title.className = "docs-search-result-title";
            title.textContent = result.page.title;
            link.appendChild(title);

            if (result.section) {
                const section = document.createElement("span");
                section.className = "docs-search-result-section";
                section.textContent = `§ ${result.section.text}`;
                link.appendChild(section);
            }

            const snippet = document.createElement("span");
            snippet.className = "docs-search-result-snippet";
            snippet.textContent = result.snippet;
            link.appendChild(snippet);
            resultsBox.appendChild(link);
        });
    };

    const search = async (query, version) => {
        if (!query.trim()) {
            resultsBox.replaceChildren();
            return;
        }

        showMessage("Buscando…");
        if (!indexPromise) indexPromise = buildIndex();
        const pages = await indexPromise;
        if (version !== searchVersion) return;
        renderResults(rankedResults(pages, query));
    };

    input.addEventListener("input", () => {
        searchVersion += 1;
        search(input.value, searchVersion);
    });

    input.addEventListener("keydown", (event) => {
        if (event.key === "Escape") {
            input.value = "";
            searchVersion += 1;
            resultsBox.replaceChildren();
            input.blur();
        }
    });

    document.addEventListener("keydown", (event) => {
        const target = event.target;
        const isTyping = target instanceof HTMLInputElement
            || target instanceof HTMLTextAreaElement
            || target.isContentEditable;
        if (event.key === "/" && !isTyping) {
            event.preventDefault();
            input.focus();
            input.select();
        }
    });
});
