/**
 * Global escape-key stack for layered overlays (Modal, Drawer, etc.).
 *
 * Each overlay pushes its close handler when it mounts/becomes visible
 * and removes it on unmount/hide. When ESC is pressed, only the
 * topmost handler fires — giving correct LIFO (last-in-first-out) behavior.
 */

type EscapeHandler = () => void;

const stack: EscapeHandler[] = [];
let listenerAttached = false;

function handleKeyDown(e: KeyboardEvent) {
  if (e.key === "Escape" && stack.length > 0) {
    e.preventDefault();
    const handler = stack[stack.length - 1];
    handler();
  }
}

export function pushEscapeHandler(handler: EscapeHandler) {
  if (!listenerAttached) {
    document.addEventListener("keydown", handleKeyDown);
    listenerAttached = true;
  }
  stack.push(handler);
}

export function removeEscapeHandler(handler: EscapeHandler) {
  const idx = stack.indexOf(handler);
  if (idx !== -1) stack.splice(idx, 1);
  if (stack.length === 0 && listenerAttached) {
    document.removeEventListener("keydown", handleKeyDown);
    listenerAttached = false;
  }
}
