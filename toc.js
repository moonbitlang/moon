// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item "><a href="tutorial.html"><strong aria-hidden="true">1.</strong> Tutorial</a></li><li class="chapter-item "><a href="commands.html"><strong aria-hidden="true">2.</strong> Moon Commands</a></li><li class="chapter-item "><a href="module.html"><strong aria-hidden="true">3.</strong> Module Configuration</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="module/name.html"><strong aria-hidden="true">3.1.</strong> name</a></li><li class="chapter-item "><a href="module/version.html"><strong aria-hidden="true">3.2.</strong> version</a></li><li class="chapter-item "><a href="module/deps.html"><strong aria-hidden="true">3.3.</strong> deps</a></li><li class="chapter-item "><a href="module/index.html"><strong aria-hidden="true">3.4.</strong> readme</a></li><li class="chapter-item "><a href="module/repository.html"><strong aria-hidden="true">3.5.</strong> repository</a></li><li class="chapter-item "><a href="module/license.html"><strong aria-hidden="true">3.6.</strong> license</a></li><li class="chapter-item "><a href="module/keywords.html"><strong aria-hidden="true">3.7.</strong> keywords</a></li><li class="chapter-item "><a href="module/description.html"><strong aria-hidden="true">3.8.</strong> description</a></li><li class="chapter-item "><a href="module/source.html"><strong aria-hidden="true">3.9.</strong> source</a></li><li class="chapter-item "><a href="package/warnings.html"><strong aria-hidden="true">3.10.</strong> warn-list</a></li><li class="chapter-item "><a href="package/alerts.html"><strong aria-hidden="true">3.11.</strong> alert-list</a></li></ol></li><li class="chapter-item "><a href="package.html"><strong aria-hidden="true">4.</strong> Package Configuration</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="package/name.html"><strong aria-hidden="true">4.1.</strong> name</a></li><li class="chapter-item "><a href="package/is-main.html"><strong aria-hidden="true">4.2.</strong> is-main</a></li><li class="chapter-item "><a href="package/import.html"><strong aria-hidden="true">4.3.</strong> import</a></li><li class="chapter-item "><a href="package/test-import.html"><strong aria-hidden="true">4.4.</strong> test-import</a></li><li class="chapter-item "><a href="package/wbtest-import.html"><strong aria-hidden="true">4.5.</strong> wbtest-import</a></li><li class="chapter-item "><a href="package/link.html"><strong aria-hidden="true">4.6.</strong> link</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="package/link/wasm.html"><strong aria-hidden="true">4.6.1.</strong> wasm</a></li><li class="chapter-item "><a href="package/link/wasm-gc.html"><strong aria-hidden="true">4.6.2.</strong> wasm-gc</a></li><li class="chapter-item "><a href="package/link/js.html"><strong aria-hidden="true">4.6.3.</strong> js</a></li></ol></li><li class="chapter-item "><a href="package/warnings.html"><strong aria-hidden="true">4.7.</strong> warn-list</a></li><li class="chapter-item "><a href="package/alerts.html"><strong aria-hidden="true">4.8.</strong> alert-list</a></li><li class="chapter-item "><a href="package/conditional-compilation.html"><strong aria-hidden="true">4.9.</strong> targets</a></li><li class="chapter-item "><a href="package/pre-build.html"><strong aria-hidden="true">4.10.</strong> pre-build</a></li></ol></li><li class="chapter-item "><a href="json_schema.html"><strong aria-hidden="true">5.</strong> JSON Schema</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
