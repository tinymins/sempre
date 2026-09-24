const locks = new Set<symbol>();
let previousOverflow = "";

export function lockBodyScroll() {
  const lock = Symbol();

  if (locks.size === 0) {
    previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
  }
  locks.add(lock);

  return () => {
    if (!locks.delete(lock) || locks.size > 0) return;
    document.body.style.overflow = previousOverflow;
  };
}
