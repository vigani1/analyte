// pages.js: the app runs this in the engine pages' webview at the start of
// every page, before the page's own scripts (ADR 0019, window).
//
// WebView2 sends a link's request before the app's navigation check cancels
// it, so a link that leaves the window used to reach its site from the
// window, with the window's cookies; one to another port on 127.0.0.1 carried
// the admin session, because cookies ignore the port. A click on a link to
// another origin therefore becomes a new-window request, which the app
// answers before anything is sent: the engine's pages and the app's own open
// in the window, the web in the default browser, anything else nowhere.
'use strict';
addEventListener('click', function (event) {
  if (event.defaultPrevented || event.button !== 0 || !(event.target instanceof Element)) return;
  var link = event.target.closest('a[href], area[href]');
  // A download stays a download; a link meant for another window already asks the app.
  if (!link || link.hasAttribute('download')) return;
  var target = (link.getAttribute('target') || '_self').toLowerCase();
  if (target !== '_self' && target !== '_top' && target !== '_parent') return;
  var url;
  try {
    url = new URL(link.getAttribute('href'), document.baseURI);
  } catch (error) {
    return;
  }
  // Same origin, or a scheme with no request to send: the app's check decides.
  if (url.origin === location.origin || (url.protocol !== 'http:' && url.protocol !== 'https:')) return;
  event.preventDefault();
  window.open(url.href, '_blank', 'noopener');
});
