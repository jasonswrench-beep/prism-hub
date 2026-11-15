// Small DOM helpers. Text always goes in as text, never as HTML.

type Attrs = Record<string, string | boolean | number | null | undefined>;

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: (Node | string | null | undefined | false)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") e.className = String(v);
    else if (k === "text") e.textContent = String(v);
    else e.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c);
  }
  return e;
}

export function $(id: string): HTMLElement {
  const e = document.getElementById(id);
  if (!e) throw new Error(`missing #${id}`);
  return e;
}

export function size(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} bytes`;
}

export function ago(secs: number): string {
  const d = Math.max(0, Math.floor(Date.now() / 1000) - secs);
  if (d < 60) return "just now";
  const m = Math.floor(d / 60);
  if (m < 60) return m === 1 ? "1 minute ago" : `${m} minutes ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return h === 1 ? "1 hour ago" : `${h} hours ago`;
  const days = Math.floor(h / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

let announceTimer = 0;
/** Read out a short message to screen readers (the live region is polite). */
export function announce(text: string): void {
  const a = $("announcer");
  a.textContent = "";
  window.clearTimeout(announceTimer);
  announceTimer = window.setTimeout(() => (a.textContent = text), 60);
}

/**
 * A menu button: opens the menu, moves focus with the arrow keys, Home and End, and closes on
 * Escape, Tab or a click outside (focus returns to the button).
 */
export interface MenuItem {
  label: string;
  run: () => void;
  separatorBefore?: boolean;
}

let openMenu: { menu: HTMLElement; button: HTMLElement } | null = null;

export function menuIsOpen(): boolean {
  return openMenu !== null;
}

export function closeMenu(returnFocus = true): void {
  if (!openMenu) return;
  const { menu, button } = openMenu;
  openMenu = null;
  menu.hidden = true;
  menu.replaceChildren();
  button.setAttribute("aria-expanded", "false");
  if (returnFocus) button.focus();
  document.dispatchEvent(new Event("menuclosed"));
}

export function showMenu(button: HTMLElement, menu: HTMLElement, items: MenuItem[], focusLast = false): void {
  closeMenu(false);
  menu.replaceChildren();
  const buttons: HTMLButtonElement[] = [];
  for (const it of items) {
    if (it.separatorBefore && buttons.length) menu.append(el("div", { role: "separator" }));
    const b = el("button", { type: "button", role: "menuitem", tabindex: "-1", text: it.label });
    b.addEventListener("click", () => {
      closeMenu();
      it.run();
    });
    buttons.push(b);
    menu.append(b);
  }
  menu.hidden = false;
  button.setAttribute("aria-expanded", "true");
  openMenu = { menu, button };
  const focusAt = (i: number) => buttons[(i + buttons.length) % buttons.length]?.focus();
  focusAt(focusLast ? buttons.length - 1 : 0);
  menu.onkeydown = (e: KeyboardEvent) => {
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown") focusAt(i + 1);
    else if (e.key === "ArrowUp") focusAt(i - 1);
    else if (e.key === "Home") focusAt(0);
    else if (e.key === "End") focusAt(buttons.length - 1);
    else if (e.key === "Escape") closeMenu();
    else if (e.key === "Tab") closeMenu(false);
    else return;
    e.preventDefault();
  };
}

document.addEventListener("mousedown", (e) => {
  if (openMenu && !openMenu.menu.contains(e.target as Node) && !openMenu.button.contains(e.target as Node)) closeMenu(false);
});

/** Wire a button to toggle a menu built on demand. */
export function menuButton(button: HTMLElement, menu: HTMLElement, items: () => MenuItem[]): void {
  button.addEventListener("click", () => {
    if (openMenu?.button === button) closeMenu();
    else showMenu(button, menu, items());
  });
  button.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      showMenu(button, menu, items(), e.key === "ArrowUp");
    }
  });
}

/** A modal question. The first button is the default and gets focus; Escape picks it too. */
export function ask(title: string, text: string, buttons: { label: string; primary?: boolean; value: string }[]): Promise<string> {
  const d = $("dialog") as HTMLDialogElement;
  $("dialog-title").textContent = title;
  $("dialog-text").textContent = text;
  const row = $("dialog-buttons");
  row.replaceChildren();
  return new Promise((resolve) => {
    const done = (v: string) => {
      d.close();
      resolve(v);
    };
    for (const b of buttons) {
      const btn = el("button", { type: "button", class: b.primary ? "btn" : "btn secondary", text: b.label });
      btn.addEventListener("click", () => done(b.value));
      row.append(btn);
    }
    d.oncancel = (e) => {
      e.preventDefault();
      done(buttons[0].value);
    };
    d.showModal();
    (row.firstElementChild as HTMLElement | null)?.focus();
  });
}
