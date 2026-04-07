// Mermaid: run diagrams after markdown is injected (Alpine x-html, chat, EasyMDE preview).
'use strict';

var _openfangMermaidInit = false;
var _openfangMermaidObserverTimer = null;

function openfangMermaidTheme() {
  var t = document.body && document.body.getAttribute('data-theme');
  if (t === 'dark') return 'dark';
  if (t === 'light') return 'default';
  if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) return 'dark';
  return 'default';
}

function ensureMermaidInitialized() {
  if (_openfangMermaidInit || typeof mermaid === 'undefined') return;
  try {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: openfangMermaidTheme(),
    });
    _openfangMermaidInit = true;
  } catch (e) {
    /* ignore */
  }
}

/** Run Mermaid on pending `.of-mermaid-pending` nodes under root (default: document). */
function renderMermaidIn(root) {
  if (typeof mermaid === 'undefined' || typeof mermaid.run !== 'function') return;
  var scope = root && root.querySelectorAll ? root : document;
  var nodes = scope.querySelectorAll('.of-mermaid-pending');
  if (!nodes.length) return;
  ensureMermaidInitialized();
  try {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: openfangMermaidTheme(),
    });
  } catch (e2) {
    /* ignore */
  }
  var list = Array.prototype.slice.call(nodes);
  list.forEach(function (el) {
    el.classList.remove('of-mermaid-pending');
  });
  mermaid.run({ nodes: list, suppressErrors: true }).catch(function () {});
}

document.addEventListener('alpine:init', function () {
  var obs = new MutationObserver(function () {
    if (!document.querySelector('.of-mermaid-pending')) return;
    clearTimeout(_openfangMermaidObserverTimer);
    _openfangMermaidObserverTimer = setTimeout(function () {
      renderMermaidIn(document.body);
    }, 80);
  });
  obs.observe(document.body, { childList: true, subtree: true });
});
