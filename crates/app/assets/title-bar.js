// title-bar.js: the window buttons of the Analyte app's own title bar (ADR 0019).
//
// The bar is the app's own page in a webview of its own, the only webview
// allowed any Tauri command, and only the window commands used here and by
// Tauri's data-tauri-drag-region (drag, and double-click to maximize). The
// engine's pages, below it in another webview, are allowed none.
//
// The bar asks the app nothing: the app calls analyteTitleBar(view) with the
// engine's state and whether the window is maximized, every word already
// written. A right-click shows no web page menu (Back, Reload, Inspect).
'use strict';
(function () {
  var bar = document.querySelector('.title-bar');
  if (!bar) return;
  var commands = {
    minimize: 'plugin:window|minimize',
    'toggle-maximize': 'plugin:window|toggle_maximize',
    close: 'plugin:window|close'
  };

  bar.addEventListener('click', function (event) {
    var button = event.target.closest('[data-window]');
    var command = button && commands[button.getAttribute('data-window')];
    var ipc = window.__TAURI_INTERNALS__;
    if (!command || !ipc) return;
    ipc.invoke(command).catch(function (error) {
      console.error('Analyte title bar: ' + command + ' failed', error);
    });
  });

  document.addEventListener('contextmenu', function (event) {
    event.preventDefault();
  });

  window.analyteTitleBar = function (view) {
    var state = bar.querySelector('[data-bar-state]');
    var toggle = bar.querySelector('[data-window="toggle-maximize"]');
    bar.setAttribute('data-sizing', view.sizing);
    toggle.setAttribute('aria-label', view.button);
    toggle.title = view.button;
    state.setAttribute('aria-label', view.label);
    state.title = view.word;
    state.firstElementChild.className = view.dot;
  };
})();
