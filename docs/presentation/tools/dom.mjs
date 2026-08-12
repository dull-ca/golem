// @excalidraw/excalidraw ships as a browser bundle: importing it touches window,
// document, canvas and fonts at module scope, so under Node or Bun it throws before
// restore() is reachable. This is the minimum surface that gets that import through
// — a jsdom window on globalThis plus a canvas 2d context that answers every call.
// It is not an attempt to render anything; measureText and friends return whatever
// keeps the bundle moving, because only restore()'s own validation is being tested.
import { JSDOM } from "jsdom";
const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true, url: "http://localhost/" });
const w = dom.window;
globalThis.window = w;
globalThis.document = w.document;
globalThis.navigator = w.navigator;
globalThis.location = w.location;
globalThis.HTMLElement = w.HTMLElement;
globalThis.HTMLCanvasElement = w.HTMLCanvasElement;
globalThis.Element = w.Element;
globalThis.Node = w.Node;
globalThis.Image = w.Image;
globalThis.DOMParser = w.DOMParser;
globalThis.getComputedStyle = w.getComputedStyle;
globalThis.matchMedia = w.matchMedia || (() => ({ matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
w.matchMedia = globalThis.matchMedia;
globalThis.requestAnimationFrame = w.requestAnimationFrame || ((cb) => setTimeout(cb, 0));
globalThis.cancelAnimationFrame = w.cancelAnimationFrame || clearTimeout;
globalThis.ResizeObserver = class { observe(){} unobserve(){} disconnect(){} };
w.ResizeObserver = globalThis.ResizeObserver;
globalThis.FontFace = class { constructor(){} load(){ return Promise.resolve(this); } };
w.FontFace = globalThis.FontFace;
if (!w.document.fonts) { w.document.fonts = { add(){}, load(){ return Promise.resolve([]); }, ready: Promise.resolve(), addEventListener(){}, check(){ return true; } }; }
globalThis.devicePixelRatio = 1;
w.devicePixelRatio = 1;
globalThis.screen = w.screen;
globalThis.localStorage = w.localStorage;
globalThis.sessionStorage = w.sessionStorage;
globalThis.MutationObserver = w.MutationObserver;
globalThis.IntersectionObserver = class { observe(){} unobserve(){} disconnect(){} };
globalThis.self = w;
globalThis.top = w;

const canvasContextStub = () => {
  const noop = () => {};
  const base = {
    filter: "none", font: "", fillStyle: "", strokeStyle: "", lineWidth: 1,
    globalAlpha: 1, globalCompositeOperation: "source-over", textAlign: "left",
    textBaseline: "alphabetic", canvas: null, imageSmoothingEnabled: true,
    measureText: (t) => ({ width: String(t).length * 8, actualBoundingBoxAscent: 8, actualBoundingBoxDescent: 2, fontBoundingBoxAscent: 8, fontBoundingBoxDescent: 2 }),
    getImageData: (x, y, wd, ht) => ({ data: new Uint8ClampedArray(Math.max(1, wd | 0) * Math.max(1, ht | 0) * 4), width: wd | 0, height: ht | 0 }),
    createImageData: (wd, ht) => ({ data: new Uint8ClampedArray(Math.max(1, wd | 0) * Math.max(1, ht | 0) * 4), width: wd | 0, height: ht | 0 }),
    createLinearGradient: () => ({ addColorStop: noop }),
    createRadialGradient: () => ({ addColorStop: noop }),
    createPattern: () => null,
    getTransform: () => ({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }),
    isPointInPath: () => false, isPointInStroke: () => false,
  };
  return new Proxy(base, { get: (target, prop) => (prop in target ? target[prop] : noop), set: (target, prop, value) => ((target[prop] = value), true) });
};
w.HTMLCanvasElement.prototype.getContext = function (kind) { return kind === "2d" ? canvasContextStub() : null; };
w.HTMLCanvasElement.prototype.toDataURL = () => "data:image/png;base64,";
w.HTMLCanvasElement.prototype.toBlob = (cb) => cb(null);
globalThis.OffscreenCanvas = class { constructor(width, height) { this.width = width; this.height = height; } getContext(kind) { return kind === "2d" ? canvasContextStub() : null; } };
w.OffscreenCanvas = globalThis.OffscreenCanvas;
