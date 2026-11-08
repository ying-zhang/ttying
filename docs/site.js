document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((item) => item.classList.remove("active"));
    tab.classList.add("active");
    ["win", "linux", "android"].forEach((name) => {
      const panel = document.getElementById("code-" + name);
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
