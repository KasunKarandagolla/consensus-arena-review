// Behavioral regression fixtures for composer-owned Send discovery.
//
// This harness extracts the REAL GENERIC_INIT_SCRIPT emitted by
// browser_backend.rs, evaluates it against a minimal fake DOM, and asserts the
// ownership contract:
//   1. A sidebar Send button and a transcript Submit/Send button must never win
//      over the composer-owned Send button.
//   2. When the composer is replaced between retry attempts, the submit action
//      must re-resolve the NEW composer and NEW Send control.
//   3. When no composer root can be resolved, the result is composer_not_found,
//      never a global fallback click or a false success.
//   4. Once a prompt is injected, the ACTIVE composer is the editable holding
//      that text: a transcript editor (earlier in DOM) or a hidden editor must
//      never win over it, and a stale/cleared input is re-resolved fresh.
//   5. The composer boundary stays NARROW: an ancestor whose class merely
//      contains "chat" must never own unrelated Send controls.
//
// Run: node src-tauri/tests/browser-ownership-fixtures.mjs
// Exit code 0 = all fixtures pass; non-zero = regression.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));

// ── Extract the emitted GENERIC_INIT_SCRIPT from the Rust source ─────────────
function extractInitScript() {
  const rsPath = process.env.CA_BROWSER_SRC || join(__dirname, '..', 'src', 'browser_backend.rs');
  const rs = readFileSync(rsPath, 'utf8');
  const marker = 'pub const GENERIC_INIT_SCRIPT: &str = r#"';
  const start = rs.indexOf(marker);
  if (start < 0) throw new Error('GENERIC_INIT_SCRIPT marker not found');
  const bodyStart = start + marker.length;
  const end = rs.indexOf('"#', bodyStart);
  if (end < 0) throw new Error('GENERIC_INIT_SCRIPT end delimiter not found');
  return rs.slice(bodyStart, end);
}

const INIT_SCRIPT = extractInitScript();

// ── Minimal CSS selector matcher (sufficient for the emitted selectors) ──────
function splitGroups(sel) {
  return sel.split(',').map((s) => s.trim()).filter(Boolean);
}

// Parse a compound selector like `button[aria-label*="send" i]` into simple parts.
function parseCompound(compound) {
  const parts = [];
  const re = /(\*?)([a-zA-Z][\w-]*)|(\#[\w-]+)|(\[[^\]]+\])/g;
  let m;
  const simple = { tag: null, attrs: [] };
  while ((m = re.exec(compound)) !== null) {
    if (m[2]) simple.tag = m[2].toLowerCase();
    else if (m[3]) simple.attrs.push({ name: 'id', op: '=', value: m[3].slice(1), flag: '' });
    else if (m[4]) {
      const inner = m[4].slice(1, -1).trim();
      const am = inner.match(/^([\w-]+)(?:\s*([~|^$*]?=)\s*"?([^"]*)"?)?(?:\s+(i|s))?$/);
      if (!am) throw new Error('unparseable attribute selector: ' + inner);
      simple.attrs.push({ name: am[1], op: am[2] || null, value: am[3] || '', flag: am[4] || '' });
    }
  }
  if (simple.tag) parts.push({ tag: simple.tag, attrs: [] });
  // distribute attrs onto the last (tag) part
  for (const a of simple.attrs) {
    if (parts.length === 0) parts.push({ tag: null, attrs: [] });
    parts[parts.length - 1].attrs.push(a);
  }
  return parts;
}

function attrValue(el, name) {
  if (name === 'class') return el.className || '';
  return el.getAttribute(name) ?? '';
}

function matchesSimple(el, simple) {
  if (simple.tag && el.tagName.toLowerCase() !== simple.tag) return false;
  for (const a of simple.attrs) {
    let val = attrValue(el, a.name);
    const expect = a.value;
    if (a.flag === 'i') { val = val.toLowerCase(); }
    if (a.op === '=' || a.op === null) {
      if (a.op === null) { if (val === '' && a.value === '') continue; if (val === '') return false; continue; }
      let e = expect;
      if (a.flag === 'i') e = e.toLowerCase();
      if (val !== e) return false;
    } else if (a.op === '*=') {
      let e = expect;
      if (a.flag === 'i') e = e.toLowerCase();
      if (!val.includes(e)) return false;
    } else if (a.op === '^=') {
      let e = expect;
      if (a.flag === 'i') e = e.toLowerCase();
      if (!val.startsWith(e)) return false;
    } else if (a.op === '$=') {
      let e = expect;
      if (a.flag === 'i') e = e.toLowerCase();
      if (!val.endsWith(e)) return false;
    }
  }
  return true;
}

function splitCompounds(group) {
  const parts = [];
  let current = '';
  let inBracket = false;
  for (let i = 0; i < group.length; i++) {
    const ch = group[i];
    if (ch === '[') inBracket = true;
    else if (ch === ']') inBracket = false;
    if (/\s/.test(ch) && !inBracket) {
      if (current.trim()) parts.push(current.trim());
      current = '';
    } else {
      current += ch;
    }
  }
  if (current.trim()) parts.push(current.trim());
  return parts;
}

function matchesSelector(el, selector) {
  for (const group of splitGroups(selector)) {
    const compounds = splitCompounds(group);
    const last = parseCompound(compounds[compounds.length - 1]);
    const simple = last[last.length - 1];
    if (!matchesSimple(el, simple)) continue;
    // verify ancestor chain for descendant combinators
    let anc = el.parentElement;
    let ok = true;
    for (let i = compounds.length - 2; i >= 0; i--) {
      const s = parseCompound(compounds[i]);
      const target = s[s.length - 1];
      let found = false;
      while (anc) {
        if (matchesSimple(anc, target)) { found = true; anc = anc.parentElement; break; }
        anc = anc.parentElement;
      }
      if (!found) { ok = false; break; }
    }
    if (ok) return true;
  }
  return false;
}

// ── Minimal fake DOM ──────────────────────────────────────────────────────────
class FakeNode {
  constructor(tagName, attrs = {}, parent = null) {
    this.tagName = tagName.toUpperCase();
    this._attrs = new Map(Object.entries(attrs));
    this.parent = parent;
    this.children = [];
    this._disabled = false;
    this._style = { display: 'block', visibility: 'visible' };
    this._rect = { top: 0, bottom: 32, left: 0, right: 64, width: 64, height: 32 };
    this._listeners = {};
    this.value = '';
    this.textContent = '';
    this.innerText = '';
    this._clicked = 0;
    this.isConnected = true;
    this._onclick = null;
    if (parent) parent.children.push(this);
  }
  get id() { return this._attrs.get('id') || ''; }
  getAttribute(name) { return this._attrs.get(name) ?? null; }
  setAttribute(name, value) { this._attrs.set(name, String(value)); }
  removeAttribute(name) { this._attrs.delete(name); }
  get disabled() { return this._disabled || this._attrs.has('disabled'); }
  set disabled(v) { this._disabled = !!v; }
  get className() { return this._attrs.get('class') || ''; }
  set className(v) { this._attrs.set('class', String(v)); }
  getBoundingClientRect() { return { ...this._rect }; }
  get parentElement() { return this.parent; }
  get style() { return this._style; }
  contains(other) { let n = other; while (n) { if (n === this) return true; n = n.parent; } return false; }
  focus() {}
  click() { this._clicked++; if (this._onclick) this._onclick(); }
  addEventListener(type, fn) { (this._listeners[type] = this._listeners[type] || []).push(fn); }
  removeEventListener() {}
  dispatchEvent() { return true; }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
  querySelectorAll(sel) {
    const out = [];
    const walk = (n) => {
      for (const child of n.children) {
        if (matchesSelector(child, sel)) out.push(child);
        walk(child);
      }
    };
    walk(this);
    return out;
  }
  matches(sel) { return matchesSelector(this, sel); }
  closest(sel) { let n = this; while (n) { if (matchesSelector(n, sel)) return n; n = n.parent; } return null; }
}

// ── Sandbox globals ───────────────────────────────────────────────────────────
let queuedTimers = [];
function makeSandbox() {
  const body = new FakeNode('body', {});
  const doc = {
    readyState: 'complete',
    title: 'fixture',
    body,
    documentElement: body,
    querySelectorAll(sel) {
      const out = [];
      if (matchesSelector(body, sel)) out.push(body);
      return out.concat(body.querySelectorAll(sel));
    },
    querySelector(sel) { return doc.querySelectorAll(sel)[0] || null; },
    addEventListener() {},
    createRange() { return { selectNodeContents() {}, setStart() {}, setEnd() {} }; },
    execCommand() { return false; },
  };
  const windowObj = {
    name: '__consensus_arena_agent__:chatgpt',
    __ca_agentId: 'chatgpt',
    __ca_ready: false,
    __ca_lastResponse: '',
    __ca_lastTurn: 0,
    location: { href: '', origin: 'https://fixture.local', pathname: '/' },
    getComputedStyle(el) { return el ? (el._style || { display: 'block', visibility: 'visible' }) : { display: 'block', visibility: 'visible' }; },
    addEventListener() {},
  };
  const sandbox = {
    window: windowObj,
    document: doc,
    Element: FakeNode,
    HTMLElement: FakeNode,
    HTMLTextAreaElement: FakeNode,
    getComputedStyle: windowObj.getComputedStyle,
    setTimeout(fn) { queuedTimers.push({ fn }); return queuedTimers.length; },
    setInterval() { return 0; },
    clearTimeout() {},
    clearInterval() {},
    MutationObserver: class { observe() {} disconnect() {} },
    Event: class { constructor(type, opts) { this.type = type; this.bubbles = !!(opts && opts.bubbles); } },
    KeyboardEvent: class { constructor(type, opts) { this.type = type; this.bubbles = !!(opts && opts.bubbles); } },
    InputEvent: class { constructor(type, opts) { this.type = type; this.bubbles = !!(opts && opts.bubbles); } },
    performance: { timeOrigin: 0 },
    console,
    encodeURIComponent,
  };
  return sandbox;
}

function runInitScript() {
  queuedTimers = [];
  const sandbox = makeSandbox();
  const script = new Function('window', 'document', 'Element', 'HTMLElement', 'HTMLTextAreaElement', 'getComputedStyle', 'setTimeout', 'setInterval', 'clearTimeout', 'clearInterval', 'MutationObserver', 'Event', 'KeyboardEvent', 'InputEvent', 'performance', 'console', 'encodeURIComponent', INIT_SCRIPT);
  script(
    sandbox.window, sandbox.document, sandbox.Element, sandbox.HTMLElement,
    sandbox.HTMLTextAreaElement, sandbox.getComputedStyle, sandbox.setTimeout,
    sandbox.setInterval, sandbox.clearTimeout, sandbox.clearInterval,
    sandbox.MutationObserver, sandbox.Event, sandbox.KeyboardEvent,
    sandbox.InputEvent, sandbox.performance, sandbox.console, sandbox.encodeURIComponent
  );
  // Drop the load-time readiness timers; fixtures drive submit timers manually.
  queuedTimers = [];
  return { window: sandbox.window, document: sandbox.document, timers: () => queuedTimers };
}

function flushNextTimer(state) {
  const t = state.timers().shift();
  if (t) t.fn();
}

// ── Fixture helpers ───────────────────────────────────────────────────────────
function composer(textarea, sendBtn) {
  const form = new FakeNode('form', { class: 'composer-shell' });
  form._rect = { top: 400, bottom: 460, left: 0, right: 800, width: 800, height: 60 };
  if (textarea) {
    form.children.push(textarea);
    textarea.parent = form;
  }
  if (sendBtn) {
    form.children.push(sendBtn);
    sendBtn.parent = form;
  }
  return form;
}

function attach(root, node) {
  root.children.push(node);
  node.parent = root;
}

function detach(node) {
  if (node.parent) {
    const idx = node.parent.children.indexOf(node);
    if (idx >= 0) node.parent.children.splice(idx, 1);
    node.parent = null;
  }
  const unflag = (n) => {
    n.isConnected = false;
    for (const child of n.children) unflag(child);
  };
  unflag(node);
}

function makeTextarea() {
  const ta = new FakeNode('textarea', { placeholder: 'Message' });
  ta._style = { display: 'block', visibility: 'visible' };
  ta._rect = { top: 404, bottom: 436, left: 0, right: 640, width: 640, height: 32 };
  ta.isConnected = true;
  return ta;
}

function makeSend(extra = {}) {
  const btn = new FakeNode('button', Object.assign({ 'aria-label': 'Send prompt' }, extra.attrs || {}));
  btn._style = { display: 'block', visibility: 'visible' };
  btn._rect = { top: 404, bottom: 436, left: 660, right: 700, width: 40, height: 32 };
  if (extra.iconOnly) {
    const svg = new FakeNode('svg', {});
    const path = new FakeNode('path', { d: 'M12 4 L20 12 L12 20 M20 12 H4' });
    attach(svg, path);
    attach(btn, svg);
  }
  if (extra.disabled) btn.disabled = true;
  btn.isConnected = true;
  return btn;
}

let failures = 0;
function assert(cond, msg) {
  if (cond) {
    console.log('  PASS: ' + msg);
  } else {
    failures++;
    console.error('  FAIL: ' + msg);
  }
}

// ── Fixture 1: sidebar + transcript must never win over composer Send ────────
function fixture1() {
  console.log('[fixture 1] sidebar/transcript/composer ownership');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // sidebar fake Send button (aria-label "Send", outside any composer)
  const sidebar = new FakeNode('aside', { class: 'sidebar' });
  const sidebarSend = new FakeNode('button', { 'aria-label': 'Send feedback' });
  sidebarSend._style = { display: 'block', visibility: 'visible' };
  attach(sidebar, sidebarSend);
  attach(body, sidebar);

  // transcript fake Submit + Send buttons inside a message article
  const article = new FakeNode('article', { class: 'message' });
  const transcriptSubmit = new FakeNode('button', { 'data-testid': 'submit' });
  transcriptSubmit._style = { display: 'block', visibility: 'visible' };
  const transcriptSend = new FakeNode('button', { 'aria-label': 'Send message' });
  transcriptSend._style = { display: 'block', visibility: 'visible' };
  attach(article, transcriptSubmit);
  attach(article, transcriptSend);
  attach(body, article);

  // the real composer with its owned Send button
  const input = makeTextarea();
  const ownedSend = makeSend();
  const form = composer(input, ownedSend);
  attach(body, form);

  const found = win.__ca_findOwnedSend(input);
  assert(found === ownedSend, 'composer-owned Send button selected, not sidebar/transcript');
  assert(found !== sidebarSend && found !== transcriptSend && found !== transcriptSubmit,
    'sidebar/transcript controls excluded');

  // drive a full auto-submit; only the composer-owned button may be clicked
  win.__caSubmitActivePrompt(input, 'chatgpt', 1);
  flushNextTimer(state); // initial attempt
  flushNextTimer(state); // (no retry expected on success; drain in case)
  assert(ownedSend._clicked === 1, 'composer Send clicked exactly once');
  assert(sidebarSend._clicked === 0 && transcriptSend._clicked === 0 && transcriptSubmit._clicked === 0,
    'no sidebar/transcript control clicked');
  const href = win.location.href;
  assert(href.includes('/active-submit/chatgpt/1/1/') || href.includes('/1/1/button_click'),
    'success ActiveSubmitReport emitted (' + href + ')');
}

// ── Fixture 2: composer replaced between retry attempts ───────────────────────
function fixture2() {
  console.log('[fixture 2] composer replacement re-resolves owned Send');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // composer A: send disabled initially
  const inputA = makeTextarea();
  const sendA = makeSend({ disabled: true });
  const formA = composer(inputA, sendA);
  attach(body, formA);

  win.__caSubmitActivePrompt(inputA, 'chatgpt', 2);
  flushNextTimer(state); // attempt 1: no enabled button -> retry scheduled

  // React-style replacement: composer A removed, composer B with enabled Send
  detach(formA);
  const inputB = makeTextarea();
  const sendB = makeSend();
  const formB = composer(inputB, sendB);
  attach(body, formB);

  flushNextTimer(state); // attempt 2: must resolve composer B + sendB
  assert(sendB._clicked === 1, 'NEW composer Send clicked after replacement');
  assert(sendA._clicked === 0, 'STALE composer Send never clicked');
  assert(inputB.isConnected, 'new composer input live');
  const href = win.location.href;
  assert(href.includes('1/button_click'), 'success ack after re-resolution (' + href + ')');
}

// ── Fixture 3: no composer root -> composer_not_found, never global fallback ──
function fixture3() {
  console.log('[fixture 3] no composer root -> composer_not_found');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // bare input whose only ancestor is document.body (no composer container)
  const input = makeTextarea();
  attach(body, input);
  // a tempting global Send button far away (must NOT be clicked)
  const globalSend = makeSend();
  attach(body, globalSend);

  win.__caSubmitActivePrompt(input, 'chatgpt', 3);
  let guard = 0;
  while (state.timers().length > 0 && guard < 60) { flushNextTimer(state); guard++; }
  assert(globalSend._clicked === 0, 'no global fallback Send clicked');
  const href = win.location.href;
  assert(href.includes('composer_not_found'), 'composer_not_found reported (' + href + ')');
  assert(href.includes('/0/'), 'failure ActiveSubmitReport emitted (not success)');
}

// ── Fixture 4: injected-text proof beats transcript editor / hidden editor ───
function fixture4() {
  console.log('[fixture 4] injected-text proof picks the CURRENT composer');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // transcript editor earlier in DOM: a contenteditable inside main holding a
  // previous message, plus a fake "Submit edit" button (a Send-shaped control).
  const main = new FakeNode('main', { class: 'message-list' });
  const transcriptEditor = new FakeNode('div', { contenteditable: 'true', class: 'editor' });
  transcriptEditor.textContent = 'previous user message being edited in the transcript';
  const transcriptSubmit = new FakeNode('button', { 'aria-label': 'Submit edit' });
  transcriptSubmit._style = { display: 'block', visibility: 'visible' };
  attach(main, transcriptEditor);
  attach(main, transcriptSubmit);
  attach(body, main);

  // hidden editor + fake send: must be excluded by visibility
  const hiddenComposer = new FakeNode('div', { class: 'composer-hidden' });
  hiddenComposer._style = { display: 'none', visibility: 'hidden' };
  const hiddenTa = new FakeNode('textarea', {});
  hiddenTa._style = { display: 'none', visibility: 'hidden' };
  const hiddenSend = makeSend();
  attach(hiddenComposer, hiddenTa);
  attach(hiddenComposer, hiddenSend);
  attach(body, hiddenComposer);

  // the ACTIVE composer: a div.composer-shell (no <form>) holding the prompt
  const injected = 'Consensus Arena active prompt that only the current composer holds';
  win.__ca_lastInjectedText = injected;
  const composerShell = new FakeNode('div', { class: 'composer-shell' });
  composerShell._rect = { top: 400, bottom: 460, left: 0, right: 800, width: 800, height: 60 };
  const composerEditor = new FakeNode('div', { contenteditable: 'true', class: 'editor' });
  composerEditor.textContent = injected;
  const ownedSend = makeSend();
  attach(composerShell, composerEditor);
  attach(composerShell, ownedSend);
  attach(body, composerShell);

  // null input forces re-resolution via findInput(); the injected-text proof
  // must prefer the composer even though the transcript editor is DOM-earlier.
  win.__caSubmitActivePrompt(null, 'chatgpt', 4);
  let guard = 0;
  while (state.timers().length > 0 && guard < 60) { flushNextTimer(state); guard++; }
  assert(ownedSend._clicked === 1, 'composer Send clicked via injected-text proof');
  assert(transcriptSubmit._clicked === 0, 'transcript Submit never clicked');
  assert(hiddenSend._clicked === 0, 'hidden composer Send never clicked');
  assert(win.location.href.includes('/4/1/'), 'success ack for turn 4 (' + win.location.href + ')');

  // Control: WITHOUT the injected-text stamp the transcript editor (DOM-early)
  // wins, proving fixture 4 actually exercises the proof and the hazard is real.
  const control = runInitScript();
  const ctrl = control.window;
  const ctrlMain = new FakeNode('main', { class: 'message-list' });
  const ctrlEditor = new FakeNode('div', { contenteditable: 'true', class: 'editor' });
  ctrlEditor.textContent = 'previous user message being edited in the transcript';
  const ctrlSubmit = new FakeNode('button', { 'aria-label': 'Submit edit' });
  ctrlSubmit._style = { display: 'block', visibility: 'visible' };
  attach(ctrlMain, ctrlEditor);
  attach(ctrlMain, ctrlSubmit);
  attach(control.document.body, ctrlMain);
  const ctrlShell = new FakeNode('div', { class: 'composer-shell' });
  ctrlShell._rect = { top: 400, bottom: 460, left: 0, right: 800, width: 800, height: 60 };
  const ctrlComposerEditor = new FakeNode('div', { contenteditable: 'true', class: 'editor' });
  ctrlComposerEditor.textContent = injected;
  attach(ctrlShell, ctrlComposerEditor);
  attach(control.document.body, ctrlShell);
  ctrl.__caSubmitActivePrompt(null, 'chatgpt', 4);
  guard = 0;
  while (control.timers().length > 0 && guard < 60) { flushNextTimer(control); guard++; }
  assert(ctrlSubmit._clicked === 1 && ctrlComposerEditor.textContent,
    'control: without stamp, DOM-early transcript editor wins (hazard proven)');
}

// ── Fixture 5: broad ancestor ("chat" in class) must not own Send ─────────────
function fixture5() {
  console.log('[fixture 5] narrow composer boundary vs broad chat ancestor');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // broad ancestor whose class contains "chat" wrapping the whole conversation
  const chatWrapper = new FakeNode('div', { class: 'chat-history' });
  const article = new FakeNode('article', { class: 'message' });
  const transcriptEditor = new FakeNode('div', { contenteditable: 'true', class: 'editor' });
  transcriptEditor.textContent = 'an old assistant turn with a Send-shaped button below';
  const feedbackSend = new FakeNode('button', { 'aria-label': 'Send feedback' });
  feedbackSend._style = { display: 'block', visibility: 'visible' };
  attach(article, transcriptEditor);
  attach(article, feedbackSend);
  attach(chatWrapper, article);
  attach(body, chatWrapper);

  // the ACTIVE composer inside the same wrapper
  const injected = 'Consensus Arena prompt for fixture five';
  win.__ca_lastInjectedText = injected;
  const composerShell = new FakeNode('div', { class: 'composer-shell' });
  composerShell._rect = { top: 400, bottom: 460, left: 0, right: 800, width: 800, height: 60 };
  const composerEditor = new FakeNode('div', { contenteditable: 'true', class: 'editor' });
  composerEditor.textContent = injected;
  const ownedSend = makeSend();
  attach(composerShell, composerEditor);
  attach(composerShell, ownedSend);
  attach(chatWrapper, composerShell);

  const found = win.__ca_findOwnedSend(composerEditor);
  assert(found === ownedSend, 'owned Send resolved from narrow composer root');
  assert(found !== feedbackSend, 'unrelated Send inside chat wrapper excluded');

  win.__caSubmitActivePrompt(null, 'chatgpt', 5);
  let guard = 0;
  while (state.timers().length > 0 && guard < 60) { flushNextTimer(state); guard++; }
  assert(ownedSend._clicked === 1, 'composer Send clicked');
  assert(feedbackSend._clicked === 0, 'feedback Send inside chat ancestor never clicked');
  assert(win.location.href.includes('/5/1/'), 'success ack for turn 5 (' + win.location.href + ')');
}

// ── Fixture 6: stale input holding different text is re-resolved to composer ──
function fixture6() {
  console.log('[fixture 6] stale input holding different text re-resolved');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // stale composer A: still connected but holds unrelated text
  const injected = 'Consensus Arena prompt for fixture six';
  win.__ca_lastInjectedText = injected;
  const inputA = makeTextarea();
  inputA.value = 'unrelated stale text that must never win';
  const sendA = makeSend();
  const formA = composer(inputA, sendA);
  attach(body, formA);

  // current composer B holding the injected prompt
  const inputB = makeTextarea();
  inputB.value = injected;
  const sendB = makeSend();
  const formB = composer(inputB, sendB);
  attach(body, formB);

  // explicit stale input is passed, but currentComposerRoot must discard it
  win.__caSubmitActivePrompt(inputA, 'chatgpt', 6);
  let guard = 0;
  while (state.timers().length > 0 && guard < 60) { flushNextTimer(state); guard++; }
  assert(sendB._clicked === 1, 'CURRENT composer Send clicked');
  assert(sendA._clicked === 0, 'stale composer Send never clicked');
  assert(win.location.href.includes('/6/1/'), 'success ack for turn 6 (' + win.location.href + ')');
}

// ── Fixture 7: ChatGPT narrow text-input wrapper must not own Send ───────────
function fixture7() {
  console.log('[fixture 7] ChatGPT narrow text-input wrapper must not own Send');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // ChatGPT-style composer: the textarea is nested inside a NARROW
  // [data-testid*="input"] wrapper whose SIBLING is the Send control inside the
  // form. A one-shot closest() that lists [data-testid*="input" i] stops at the
  // wrapper (nearest match) and reports send_button_candidate_count=0 — the
  // exact live failure this harness reproduces. The wrapper is NOT a semantic
  // composer ancestor, so ownership must walk up to the form that owns Send.
  const form = new FakeNode('form', { id: 'composer-background' });
  form._rect = { top: 400, bottom: 460, left: 0, right: 800, width: 800, height: 60 };
  const wrapper = new FakeNode('div', { 'data-testid': 'text-input-container' });
  const textarea = makeTextarea();
  textarea.value = 'Consensus Arena prompt for fixture seven';
  const sendButton = new FakeNode('button', { 'data-testid': 'send-button', 'aria-label': 'Send prompt' });
  sendButton._style = { display: 'block', visibility: 'visible' };
  sendButton._rect = { top: 404, bottom: 436, left: 660, right: 700, width: 40, height: 32 };
  attach(wrapper, textarea);
  attach(form, wrapper);
  attach(form, sendButton);
  attach(body, form);

  // transcript article with a Send-shaped control: must never win
  const article = new FakeNode('article', { class: 'message' });
  const transcriptSend = new FakeNode('button', { 'aria-label': 'Send feedback' });
  transcriptSend._style = { display: 'block', visibility: 'visible' };
  attach(article, transcriptSend);
  attach(body, article);

  // Ownership proof: __ca_findOwnedSend is the same discovery the readiness
  // send-probe and the priming capability probe use (send_button_candidate_count
  // is exactly the length of its candidate list). It must resolve the
  // form-owned Send, never the transcript control, never null.
  win.__ca_lastInjectedText = 'Consensus Arena prompt for fixture seven';
  const found = win.__ca_findOwnedSend(textarea);
  assert(found === sendButton, 'owned Send resolved through narrow text-input wrapper');
  assert(found !== transcriptSend, 'transcript Send excluded');
  assert(found !== null, 'no owned Send dropped (send_button_candidate_count must be >= 1)');

  // ACTIVE path: auto-submit must click the form-owned Send through the wrapper.
  win.__caSubmitActivePrompt(textarea, 'chatgpt', 7);
  let guard = 0;
  while (state.timers().length > 0 && guard < 60) { flushNextTimer(state); guard++; }
  assert(sendButton._clicked === 1, 'composer Send clicked through narrow wrapper');
  assert(transcriptSend._clicked === 0, 'transcript Send never clicked');
  assert(win.location.href.includes('/7/1/'), 'success ack for turn 7 (' + win.location.href + ')');
}

// ── Fixture 8: arbitrary custom identity (acme) rides the generic driver ──────
// P2: proves a NON-built-in identity (a persisted custom participant) is handled
// by the existing generic GENERIC_INIT_SCRIPT with zero model-specific logic:
// the composer-owned Send is discovered, the auto-submit fires, and the
// arena://active-submit signal carries the custom id. The driver never consults
// the seven-model registry, so "acme" (or any id) works identically.
function fixture8() {
  console.log('[fixture 8] non-built-in identity acme rides the generic driver');
  const state = runInitScript();
  const { window: win, document: doc } = state;
  const body = doc.body;

  // Establish the custom identity exactly as the Rust side does for a real
  // custom participant (window.__ca_agentId is arbitrary, not a built-in id).
  win.__ca_agentId = 'acme';
  win.name = '__consensus_arena_agent__:acme';

  const input = makeTextarea();
  input.value = 'Consensus Arena active prompt for custom participant acme';
  win.__ca_lastInjectedText = input.value;
  const send = makeSend();
  const form = composer(input, send);
  attach(body, form);

  // Composer-owned Send discovery is generic — no register/list of IDs.
  const found = win.__ca_findOwnedSend(input);
  assert(found === send, 'owned Send discovered for custom identity acme');
  assert(found === send, 'acme Send is composer-owned (no built-in id required)');

  // Auto-submit with the custom id; the arena signal must carry 'acme'.
  win.__caSubmitActivePrompt(input, 'acme', 9);
  let guard = 0;
  while (state.timers().length > 0 && guard < 60) { flushNextTimer(state); guard++; }
  assert(send._clicked === 1, 'composer Send clicked for acme');
  const href = win.location.href;
  assert(href.includes('/acme/9/1/') || href.includes('active-submit/acme/9/1/'),
    'active-submit signal carries custom id acme (' + href + ')');
}

fixture1();
fixture2();
fixture3();
fixture4();
fixture5();
fixture6();
fixture7();
fixture8();

console.log(failures === 0 ? '\nALL FIXTURES PASSED' : `\n${failures} FIXTURE(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
