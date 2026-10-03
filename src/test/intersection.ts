// Controllable IntersectionObserver for jsdom (which has none). Tests call `intersectAll()`.
type Entry = { cb: IntersectionObserverCallback; els: Set<Element>; io: IntersectionObserver };
const observers = new Set<Entry>();

class FakeIntersectionObserver {
  readonly root = null;
  readonly rootMargin = "0px";
  readonly thresholds = [0];
  private entry: Entry;
  constructor(cb: IntersectionObserverCallback) {
    this.entry = { cb, els: new Set(), io: this as unknown as IntersectionObserver };
    observers.add(this.entry);
  }
  observe(el: Element) {
    this.entry.els.add(el);
  }
  unobserve(el: Element) {
    this.entry.els.delete(el);
  }
  disconnect() {
    this.entry.els.clear();
    observers.delete(this.entry);
  }
  takeRecords() {
    return [];
  }
}

export function installIntersectionObserver() {
  globalThis.IntersectionObserver = FakeIntersectionObserver as unknown as typeof IntersectionObserver;
}

/** Reports every observed element as intersecting (or leaving, with `false`). */
export function intersectAll(isIntersecting = true) {
  for (const { cb, els, io } of [...observers]) {
    const entries = [...els].map((target) => ({ target, isIntersecting }) as IntersectionObserverEntry);
    if (entries.length) cb(entries, io);
  }
}
