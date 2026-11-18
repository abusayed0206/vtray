// Enables the label-printer feed and the consent sticker peel.
// Without JavaScript everything is visible and static (the "js" class is set inline in <head>).

for (const peel of document.querySelectorAll("[data-peel]")) {
  const sticker = peel.querySelector(".peel-sticker");
  const dialog = peel.querySelector(".dialog");
  sticker.setAttribute("aria-expanded", "false");
  dialog.setAttribute("aria-hidden", "true");
  sticker.addEventListener("click", () => {
    peel.classList.add("is-peeled");
    sticker.setAttribute("aria-expanded", "true");
    dialog.removeAttribute("aria-hidden");
    dialog.setAttribute("tabindex", "-1");
    dialog.focus({ preventScroll: true });
  });
}
