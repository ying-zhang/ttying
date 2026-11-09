let currentLang = document.documentElement.dataset.lang || "en";

function applyLanguage(lang) {
  currentLang = lang === "zh" ? "zh" : "en";
  document.documentElement.dataset.lang = currentLang;
  document.documentElement.lang = currentLang === "zh" ? "zh-CN" : "en";
  document.title = currentLang === "zh" ? "ttying - 安全的 Web 终端" : "ttying - secure web terminal";
  document.querySelectorAll("[data-lang-toggle]").forEach((button) => {
    button.textContent = currentLang === "zh" ? "English" : "中文";
  });
  try { localStorage.setItem("ttying-lang", currentLang); } catch (error) {}
}

applyLanguage(currentLang);
document.querySelectorAll("[data-lang-toggle]").forEach((button) => {
  button.addEventListener("click", (event) => {
    event.preventDefault();
    applyLanguage(currentLang === "zh" ? "en" : "zh");
  });
});

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    const card = tab.closest(".quick-card");
    if (!card) return;
    card.querySelectorAll(".tab").forEach((item) => item.classList.remove("active"));
    tab.classList.add("active");
    ["win", "linux", "macos", "android"].forEach((name) => {
      const panel = card.querySelector("#code-" + name);
      if (panel) panel.hidden = name !== tab.dataset.tab;
    });
  });
});

document.querySelectorAll("[data-copy]").forEach((button) => {
  button.addEventListener("click", async () => {
    const card = button.closest(".quick-card");
    const visible = card ? [...card.querySelectorAll("pre")].find((pre) => !pre.hidden) : null;
    if (!visible) return;
    try {
      await navigator.clipboard.writeText(visible.innerText);
      const old = button.textContent;
      button.textContent = document.documentElement.lang.startsWith("zh") ? "已复制" : "Copied";
      setTimeout(() => { button.textContent = old; }, 1400);
    } catch (error) {
      button.textContent = document.documentElement.lang.startsWith("zh") ? "复制失败" : "Copy failed";
    }
  });
});
