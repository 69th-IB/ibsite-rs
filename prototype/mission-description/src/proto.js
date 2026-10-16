// PROTOTYPE helper script: variant keys, Trix upload block, Markdown toolbar.
document.addEventListener("keydown", (e) => {
  const t = e.target;
  if (t.closest("input, textarea, [contenteditable], trix-editor")) return;
  if (e.key === "ArrowLeft") document.getElementById("variant-prev")?.click();
  if (e.key === "ArrowRight") document.getElementById("variant-next")?.click();
});

// The prototype has no upload endpoint.
document.addEventListener("trix-file-accept", (e) => e.preventDefault());

document.addEventListener("click", (e) => {
  const button = e.target.closest("[data-md-wrap], [data-md-line], [data-md-link]");
  if (!button) return;
  const area = button.closest(".fields").querySelector("textarea[name=description]");
  const { selectionStart: start, selectionEnd: end, value } = area;
  const selected = value.slice(start, end);
  let insert;
  if (button.dataset.mdWrap !== undefined) {
    const mark = button.dataset.mdWrap;
    insert = mark + (selected || "text") + mark;
  } else if (button.dataset.mdLine !== undefined) {
    const prefix = button.dataset.mdLine;
    insert = (selected || "item").split("\n").map((line) => prefix + line).join("\n");
    if (start > 0 && value[start - 1] !== "\n") insert = "\n" + insert;
  } else {
    insert = "[" + (selected || "link text") + "](https://)";
  }
  area.focus();
  area.setRangeText(insert, start, end, "end");
  area.dispatchEvent(new Event("input", { bubbles: true }));
});
