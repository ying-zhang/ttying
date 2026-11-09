/* ttying landing page: language switch, stable anchors and platform tabs. */
const ANCHORS = ["quickstart", "features", "screenshots", "security", "about"];
let currentLang = document.documentElement.dataset.lang === "zh" ? "zh" : "en";
let activePlatform = "win";

function activePage() {
  return document.querySelector('.lang-page[data-lang-page="' + currentLang + '"]');
}

function hashName() {
  try {
    return decodeURIComponent(location.hash.replace(/^#/, ""));
  } catch (error) {
    return location.hash.replace(/^#/, "");
  }
}

function scrollToAnchor(name, behavior) {
  if (!name || name === "top") {
    window.scrollTo({ top: 0, behavior: behavior });
    return;
  }
  if (ANCHORS.indexOf(name) === -1) return;
  const page = activePage();
  const target = page && page.querySelector('[data-anchor="' + name + '"]');
  if (target) target.scrollIntoView({ behavior: behavior, block: "start" });
}

/* Runs the callback once the new language page has been laid out.
   A single frame still sees the previous display value, so scrollIntoView
   would target a display:none node and do nothing. */
function afterLayout(fn) {
  requestAnimationFrame(() => requestAnimationFrame(fn));
}

function applyLanguage(lang, keepPosition) {
  currentLang = lang === "zh" ? "zh" : "en";
  document.documentElement.dataset.lang = currentLang;
  document.documentElement.lang = currentLang === "zh" ? "zh-CN" : "en";
  document.title = currentLang === "zh" ? "ttying - 安全的 Web 终端" : "ttying - secure web terminal";
  document.querySelectorAll("[data-lang-toggle]").forEach((el) => {
    el.textContent = currentLang === "zh" ? "English" : "中文";
  });
  try { localStorage.setItem("ttying-lang", currentLang); } catch (error) {}
  selectPlatform(activePlatform);
  if (keepPosition) {
    const name = hashName();
    if (name) {
      /* Reading a layout property flushes style and layout so the newly
         shown language page has boxes before we measure it. */
      void document.body.offsetHeight;
      scrollToAnchor(name, "auto");
    }
  }
}

function selectPlatform(name) {
  if (["win", "linux", "macos", "android"].indexOf(name) === -1) return;
  activePlatform = name;
  document.querySelectorAll(".quick-card").forEach((card) => {
    card.querySelectorAll(".tab").forEach((tab) => {
      tab.classList.toggle("active", tab.dataset.tab === name);
    });
    card.querySelectorAll("[data-panel]").forEach((panel) => {
      panel.hidden = panel.dataset.panel !== name;
    });
  });
}

applyLanguage(currentLang, false);
document.querySelectorAll("[data-lang-toggle]").forEach((el) => {
  el.addEventListener("click", (event) => {
    event.preventDefault();
    applyLanguage(currentLang === "zh" ? "en" : "zh", true);
  });
});

document.querySelectorAll('a[href^="#"]:not([data-lang-toggle])').forEach((link) => {
  link.addEventListener("click", (event) => {
    const name = link.getAttribute("href").slice(1);
    if (!name) return;
    event.preventDefault();
    if (location.hash !== "#" + name) history.pushState(null, "", "#" + name);
    scrollToAnchor(name, "smooth");
  });
});

window.addEventListener("hashchange", () => scrollToAnchor(hashName(), "smooth"));
window.addEventListener("popstate", () => scrollToAnchor(hashName(), "auto"));
window.addEventListener("load", () => scrollToAnchor(hashName(), "auto"));

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => selectPlatform(tab.dataset.tab));
});

document.querySelectorAll("[data-copy]").forEach((button) => {
  button.addEventListener("click", async () => {
    const card = button.closest(".quick-card");
    const visible = card ? [...card.querySelectorAll("pre")].find((pre) => !pre.hidden) : null;
    if (!visible) return;
    const label = button.textContent;
    try {
      await navigator.clipboard.writeText(visible.innerText);
      button.textContent = currentLang === "zh" ? "已复制" : "Copied";
    } catch (error) {
      button.textContent = currentLang === "zh" ? "复制失败" : "Copy failed";
    }
    setTimeout(() => { button.textContent = label; }, 1400);
  });
});

if (hashName()) afterLayout(() => scrollToAnchor(hashName(), "auto"));
