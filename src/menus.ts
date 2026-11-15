// SPDX-License-Identifier: Apache-2.0
// Menus that work by keyboard as the ARIA menu button pattern describes: Enter, Space or Down opens and focuses the first item; Up and Down
// move; Home and End jump; Escape and Tab close it, and Escape puts the focus back on its button. No keyboard trap.

const open = new Set<HTMLElement>();

export function closeAllMenus(except?: HTMLElement) {
  for (const menu of [...open]) if (menu !== except) closeMenu(menu, false);
}

function items(menu: HTMLElement): HTMLButtonElement[] {
  return [...menu.querySelectorAll<HTMLButtonElement>("[role=menuitem]")].filter((b) => !b.disabled);
}

function closeMenu(menu: HTMLElement, focusButton: boolean) {
  menu.hidden = true;
  open.delete(menu);
  const button = document.querySelector<HTMLElement>(`[aria-controls="${menu.id}"]`);
  button?.setAttribute("aria-expanded", "false");
  if (focusButton) button?.focus();
}

function openMenu(button: HTMLElement, menu: HTMLElement, focus: "first" | "last") {
  closeAllMenus(menu);
  menu.hidden = false;
  open.add(menu);
  button.setAttribute("aria-expanded", "true");
  // Open upward when there is no room below (the last rows of the list).
  menu.classList.remove("above");
  const room = window.innerHeight - button.getBoundingClientRect().bottom;
  if (room < menu.offsetHeight + 16 && button.getBoundingClientRect().top > menu.offsetHeight + 16) menu.classList.add("above");
  const list = items(menu);
  (focus === "first" ? list[0] : list[list.length - 1])?.focus();
}

/** Makes `button` open `menu`; `choose` gets the chosen item's data-action. */
export function bindMenuButton(button: HTMLElement, menu: HTMLElement, choose: (action: string) => void) {
  button.addEventListener("click", () => (menu.hidden ? openMenu(button, menu, "first") : closeMenu(menu, true)));
  button.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      openMenu(button, menu, e.key === "ArrowDown" ? "first" : "last");
    }
  });
  menu.addEventListener("keydown", (e) => {
    const list = items(menu);
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    if (e.key === "ArrowDown") go(at + 1);
    else if (e.key === "ArrowUp") go(at - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(list.length - 1);
    else if (e.key === "Escape") closeMenu(menu, true);
    else if (e.key === "Tab") closeMenu(menu, false);
    else return;
    if (e.key !== "Tab") e.preventDefault();
  });
  menu.addEventListener("click", (e) => {
    const item = (e.target as HTMLElement).closest<HTMLButtonElement>("[role=menuitem]");
    if (!item) return;
    closeMenu(menu, true);
    choose(item.dataset.action ?? "");
  });
}
