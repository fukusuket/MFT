// A small DOM stand-in for the viewer tests (ADR 0022): just what viewer.js uses. Markup
// setters throw, so a test fails if the viewer ever builds HTML from strings.

class Node {
  constructor(tag) {
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.parent = null;
    this.text = "";
    this.className = "";
    this.id = "";
    this.style = {};
    this.value = "";
    this.scrollTop = 0;
    this.clientHeight = 400;
    this.listeners = {};
    this.attributes = {};
  }

  get textContent() {
    return this.text + this.children.map((c) => c.textContent).join("");
  }

  set textContent(value) {
    this.children = [];
    this.text = String(value);
  }

  set innerHTML(_) {
    throw new Error("innerHTML is not allowed");
  }

  set outerHTML(_) {
    throw new Error("outerHTML is not allowed");
  }

  insertAdjacentHTML() {
    throw new Error("insertAdjacentHTML is not allowed");
  }

  appendChild(child) {
    child.parent = this;
    this.children.push(child);
    return child;
  }

  replaceChildren(...children) {
    this.replaced = (this.replaced || 0) + 1; // lets tests count repaints
    this.children = [];
    children.forEach((c) => this.appendChild(c));
  }

  setAttribute(name, value) {
    this.attributes[name] = String(value);
  }

  getAttribute(name) {
    return name in this.attributes ? this.attributes[name] : null;
  }

  scrollIntoView(options) {
    this.scrolledIntoView = options;
  }

  addEventListener(type, listener) {
    (this.listeners[type] ||= []).push(listener);
  }

  dispatch(type) {
    (this.listeners[type] || []).forEach((l) => l({ type, target: this }));
  }

  /** Every node below this one, depth first. */
  *all() {
    for (const c of this.children) {
      yield c;
      yield* c.all();
    }
  }

  /** The first node below with this class name (one of its space-separated classes). */
  find(className) {
    for (const n of this.all()) if (n.className.split(" ").includes(className)) return n;
    return null;
  }

  findAll(className) {
    return [...this.all()].filter((n) => n.className.split(" ").includes(className));
  }
}

/** A document whose body holds elements with these ids, and a window around it. */
export function page(ids, hash = "") {
  const body = new Node("body");
  for (const [id, text] of Object.entries(ids)) {
    const node = new Node(id === "data" ? "script" : "div");
    node.id = id;
    node.text = text;
    body.appendChild(node);
  }
  const document = {
    body,
    createElement: (tag) => new Node(tag),
    getElementById: (id) => [...body.all()].find((n) => n.id === id) || null,
    write() {
      throw new Error("document.write is not allowed");
    },
  };
  const window = new Node("window");
  // As in a browser, setting a different hash adds a history entry; `replaceState` does not.
  let current = hash;
  window.history = {
    length: 1,
    replaceState(_state, _title, url) {
      current = url;
    },
  };
  window.location = {
    get hash() {
      return current;
    },
    set hash(value) {
      if (value !== current) window.history.length += 1;
      current = value;
    },
  };
  // As in a browser, a hash change fires `hashchange` (here at once).
  window.go = (newHash) => {
    window.location.hash = newHash;
    window.dispatch("hashchange");
  };
  return { document, window };
}
