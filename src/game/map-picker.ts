/** Gap between the picker and its list, and between the list and the viewport edge. */
const LIST_GAP_PX = 4;
const VIEWPORT_MARGIN_PX = 8;
/** A narrow picker's list still shows each map's description. */
const MIN_LIST_WIDTH_PX = 320;

/** Offer the extra levels in every map choice under `root`. The standard maps fit a row
 * of buttons; with the extra levels the choice becomes a dropdown. */
export function showExtraLevels(root: ParentNode): void {
  root.querySelectorAll<HTMLElement>(".map-row").forEach((row) => {
    row.hidden = true;
  });
  root.querySelectorAll<HTMLElement>(".map-picker, .map-picker-group").forEach((part) => {
    part.hidden = false;
  });
}

/** The maps a picker offers: all but the extra levels on a page without them. The picker
 * itself may be hidden behind the row of standard maps and still holds the choice. */
function offeredOptions(picker: HTMLElement): HTMLElement[] {
  return [...picker.querySelectorAll<HTMLElement>('[role="option"]')].filter(
    (option) => !option.closest<HTMLElement>(".map-picker-group")?.hidden,
  );
}

/** Show `value` as the chosen map without announcing a change. A map the picker does not
 * offer, such as an extra level on a page without them, leaves the choice as it was. */
function setMapPicker(picker: HTMLElement, value: string): boolean {
  const chosen = offeredOptions(picker).find((option) => option.dataset.value === value);
  if (!chosen) {
    return false;
  }
  picker.dataset.value = value;
  for (const option of picker.querySelectorAll('[role="option"]')) {
    option.setAttribute("aria-selected", String(option === chosen));
  }
  showInPicker(picker, chosen);
  return true;
}

/** Show `option`'s map on the closed picker. */
function showInPicker(picker: HTMLElement, option: Element): void {
  picker
    .querySelector(".map-picker-current")!
    .replaceChildren(...[...option.childNodes].map((node) => node.cloneNode(true)));
}

function mapPicker(root: ParentNode, name: string): HTMLElement | null {
  return root.querySelector<HTMLElement>(`.map-picker[data-name="${name}"]`);
}

/** Show `value` in both views of the `name` map choice. The dropdown always holds the
 * choice, even while the row of standard maps is the one shown. */
export function setMapChoice(root: ParentNode, name: string, value: string): boolean {
  const picker = mapPicker(root, name);
  if (!picker || !setMapPicker(picker, value)) {
    return false;
  }
  root.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`).forEach((input) => {
    input.checked = input.value === value;
  });
  return true;
}

/** Show `room`, the map of the open room the player chose, in place of the `name` map
 * choice; no `room` shows the choice again. The choice itself, which single player and
 * a new room play, stays as it was. */
export function showRoomMap(root: ParentNode, name: string, room?: string): void {
  const picker = mapPicker(root, name);
  if (!picker) {
    return;
  }
  root.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`).forEach((input) => {
    input.closest(".choice-card")?.classList.toggle("room-map", input.value === room);
  });
  const options = [...picker.querySelectorAll<HTMLElement>('[role="option"]')];
  // An extra level shows on the picker even on a page that does not offer it.
  const shown = room ? options.find((option) => option.dataset.value === room) : undefined;
  if (shown && room) {
    picker.dataset.roomMap = room;
  } else {
    delete picker.dataset.roomMap;
  }
  const chosen = options.find((option) => option.dataset.value === picker.dataset.value);
  showInPicker(picker, shown ?? chosen!);
}

/** Make the `name` map choice work and keep its two views in step; `change` runs with the
 * new map after the player picks one in either view. */
export function bindMapChoice(
  root: ParentNode,
  name: string,
  change: (value: string) => void,
): void {
  const picker = mapPicker(root, name);
  if (!picker) {
    return;
  }
  bindMapPicker(picker);
  picker.addEventListener("change", () => {
    setMapChoice(root, name, picker.dataset.value!);
    change(picker.dataset.value!);
  });
  root.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`).forEach((input) => {
    input.addEventListener("change", () => {
      if (input.checked && setMapChoice(root, name, input.value)) {
        change(input.value);
      }
    });
  });
}

/** Open the list below the picker if it fits whole, otherwise on the roomier side. */
function placeList(button: HTMLElement, list: HTMLElement): void {
  const rect = button.getBoundingClientRect();
  const width = Math.min(
    Math.max(rect.width, MIN_LIST_WIDTH_PX),
    innerWidth - VIEWPORT_MARGIN_PX * 2,
  );
  // Width decides how the descriptions wrap, so set it before measuring the height.
  list.style.width = `${width}px`;
  list.style.left = `${Math.max(VIEWPORT_MARGIN_PX, Math.min(rect.left, innerWidth - width - VIEWPORT_MARGIN_PX))}px`;
  const below = innerHeight - rect.bottom - LIST_GAP_PX - VIEWPORT_MARGIN_PX;
  const above = rect.top - LIST_GAP_PX - VIEWPORT_MARGIN_PX;
  const downward = below >= list.scrollHeight || below >= above;
  list.style.maxHeight = `${downward ? below : above}px`;
  list.style.top = downward ? `${rect.bottom + LIST_GAP_PX}px` : "auto";
  list.style.bottom = downward ? "auto" : `${innerHeight - rect.top + LIST_GAP_PX}px`;
}

/** Make a picker from `scripts/map-picker-markup.ts` work as a select-only combobox: focus stays on the
 * picker, arrows move through the open list, and choosing a new map fires a bubbling
 * `change` on the picker. The list opens in the top layer, so dialogs cannot clip it. */
function bindMapPicker(picker: HTMLElement): void {
  const button = picker.querySelector<HTMLElement>(".map-picker-button")!;
  const list = picker.querySelector<HTMLElement>(".map-picker-list")!;
  let active: HTMLElement | undefined;
  const isOpen = () => list.matches(":popover-open");
  const highlight = (option: HTMLElement | undefined) => {
    active?.classList.remove("active");
    active = option;
    if (option) {
      option.classList.add("active");
      button.setAttribute("aria-activedescendant", option.id);
      option.scrollIntoView({ block: "nearest" });
    } else {
      button.removeAttribute("aria-activedescendant");
    }
  };
  const outside = (event: PointerEvent) => {
    // The list renders in the top layer but stays inside the picker in the DOM.
    if (!picker.contains(event.target as Node)) {
      close();
    }
  };
  const open = (option?: HTMLElement) => {
    if (!isOpen()) {
      list.showPopover();
      placeList(button, list);
      button.setAttribute("aria-expanded", "true");
      document.addEventListener("pointerdown", outside, true);
      window.addEventListener("resize", close);
    }
    highlight(
      option ?? offeredOptions(picker).find((item) => item.dataset.value === picker.dataset.value),
    );
  };
  function close(): void {
    if (isOpen()) {
      list.hidePopover();
    }
    button.setAttribute("aria-expanded", "false");
    highlight(undefined);
    document.removeEventListener("pointerdown", outside, true);
    window.removeEventListener("resize", close);
  }
  const choose = (option: HTMLElement) => {
    close();
    const value = option.dataset.value!;
    if (value !== picker.dataset.value && setMapPicker(picker, value)) {
      picker.dispatchEvent(new Event("change", { bubbles: true }));
    }
  };
  const step = (offset: number) => {
    const options = offeredOptions(picker);
    const index = active ? options.indexOf(active) : -1;
    return options[Math.max(0, Math.min(options.length - 1, index + offset))];
  };
  button.addEventListener("click", () => (isOpen() ? close() : open()));
  button.addEventListener("blur", close);
  button.addEventListener("keydown", (event) => {
    const options = offeredOptions(picker);
    let target: HTMLElement | undefined;
    if (event.key === "ArrowDown") {
      target = isOpen() && !event.altKey ? step(1) : undefined;
    } else if (event.key === "ArrowUp") {
      target = isOpen() ? step(-1) : undefined;
    } else if (event.key === "Home" || event.key === "PageUp") {
      target = options[0];
    } else if (event.key === "End" || event.key === "PageDown") {
      target = options.at(-1);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (isOpen() && active) {
        choose(active);
      } else {
        open();
      }
      return;
    } else if (event.key === "Escape" && isOpen()) {
      // Close the list without also pausing or leaving the menu behind it.
      event.stopPropagation();
      close();
      return;
    } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      // Type-ahead: the next map whose name starts with the typed letter.
      const letter = event.key.toLowerCase();
      const start = active ? options.indexOf(active) + 1 : 0;
      target = [...options.slice(start), ...options.slice(0, start)].find((option) =>
        option.querySelector("b")?.textContent?.toLowerCase().startsWith(letter),
      );
      if (!target) {
        return;
      }
    } else {
      return;
    }
    event.preventDefault();
    open(target);
  });
  // Keep focus on the picker while the pointer chooses from its list.
  list.addEventListener("pointerdown", (event) => event.preventDefault());
  list.addEventListener("pointermove", (event) => {
    const option = (event.target as Element).closest<HTMLElement>('[role="option"]');
    if (option && option !== active) {
      highlight(option);
    }
  });
  list.addEventListener("click", (event) => {
    const option = (event.target as Element).closest<HTMLElement>('[role="option"]');
    if (option) {
      choose(option);
    }
  });
}
