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
        this.innerHTML = '<ol class="chapter"><li class="chapter-item "><a href="tutorial.html"><strong aria-hidden="true">1.</strong> 构建系统教程</a></li><li class="chapter-item "><a href="commands.html"><strong aria-hidden="true">2.</strong> moon 命令</a></li><li class="chapter-item "><a href="module.html"><strong aria-hidden="true">3.</strong> 模块配置</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="module/name.html"><strong aria-hidden="true">3.1.</strong> 模块名</a></li><li class="chapter-item "><a href="module/version.html"><strong aria-hidden="true">3.2.</strong> 版本</a></li><li class="chapter-item "><a href="module/deps.html"><strong aria-hidden="true">3.3.</strong> 依赖</a></li><li class="chapter-item "><a href="module/index.html"><strong aria-hidden="true">3.4.</strong> README 文件</a></li><li class="chapter-item "><a href="module/repository.html"><strong aria-hidden="true">3.5.</strong> 仓库地址</a></li><li class="chapter-item "><a href="module/license.html"><strong aria-hidden="true">3.6.</strong> 许可证</a></li><li class="chapter-item "><a href="module/keywords.html"><strong aria-hidden="true">3.7.</strong> 关键词</a></li><li class="chapter-item "><a href="module/description.html"><strong aria-hidden="true">3.8.</strong> 描述</a></li><li class="chapter-item "><a href="module/source.html"><strong aria-hidden="true">3.9.</strong> 源码目录</a></li><li class="chapter-item "><a href="package/warnings.html"><strong aria-hidden="true">3.10.</strong> warn 列表</a></li><li class="chapter-item "><a href="package/alerts.html"><strong aria-hidden="true">3.11.</strong> alert 列表</a></li></ol></li><li class="chapter-item "><a href="package.html"><strong aria-hidden="true">4.</strong> 包配置</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="package/name.html"><strong aria-hidden="true">4.1.</strong> 包名</a></li><li class="chapter-item "><a href="package/is-main.html"><strong aria-hidden="true">4.2.</strong> is-main 字段</a></li><li class="chapter-item "><a href="package/import.html"><strong aria-hidden="true">4.3.</strong> import 字段</a></li><li class="chapter-item "><a href="package/test-import.html"><strong aria-hidden="true">4.4.</strong> test-import 字段</a></li><li class="chapter-item "><a href="package/wbtest-import.html"><strong aria-hidden="true">4.5.</strong> wbtest-import字段</a></li><li class="chapter-item "><a href="package/link.html"><strong aria-hidden="true">4.6.</strong> 链接选项</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="package/link/wasm.html"><strong aria-hidden="true">4.6.1.</strong> wasm 后端链接选项</a></li><li class="chapter-item "><a href="package/link/wasm-gc.html"><strong aria-hidden="true">4.6.2.</strong> wasm-gc 后端链接选项</a></li><li class="chapter-item "><a href="package/link/js.html"><strong aria-hidden="true">4.6.3.</strong> js 后端链接选项</a></li></ol></li><li class="chapter-item "><a href="package/warnings.html"><strong aria-hidden="true">4.7.</strong> warn 列表</a></li><li class="chapter-item "><a href="package/alerts.html"><strong aria-hidden="true">4.8.</strong> alert 列表</a></li><li class="chapter-item "><a href="package/conditional-compilation.html"><strong aria-hidden="true">4.9.</strong> 条件编译</a></li><li class="chapter-item "><a href="package/pre-build.html"><strong aria-hidden="true">4.10.</strong> 预构建命令</a></li></ol></li><li class="chapter-item "><a href="json_schema.html"><strong aria-hidden="true">5.</strong> JSON Schema</a></li></ol>';
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
