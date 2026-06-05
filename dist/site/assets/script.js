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
