// live.js: keeps a signed-in Analyte page current without a reload (ADR 0018).
//
// Every page works fully without this script: each value is written by the
// server, and this file only fetches the same page again and swaps the parts
// marked data-live-region when the server says something it shows has changed.
// It listens to /events (server-sent events), which carry only which areas
// changed and the time; never page content, values or patient data.
//
// The sidebar says "Live · 10:45:12" while connected and goes back to "Page
// loaded 10:45" when the stream drops; it then retries after 1, 2, 4 ... 30 s.
// A region under the pointer, holding the keyboard focus or an open
// disclosure waits until the reader leaves it, so nothing moves under them.
// Motion follows the design's CSS, which turns it off for reduced motion.
'use strict';
(function () {
  var meta = document.querySelector('.engine__meta');
  var dot = document.querySelector('.engine .live-dot');
  var say = document.getElementById('live-say');
  if (!meta || !dot || !window.EventSource || !window.fetch || !window.DOMParser) return;
  var shows = (document.body.getAttribute('data-live') || '').split(' ');
  var loaded = meta.textContent;
  var wait = 1000;
  var dropped = false;
  var paused = false;
  var busy = false;
  var again = false;
  var said = '';

  function setLive(on) {
    var pill = document.querySelector('[data-live-pill]');
    var pause = document.querySelector('[data-live-pause]');
    dot.classList.toggle('live-dot--still', !on);
    if (pill) pill.hidden = !on;
    if (pause) {
      pause.hidden = !on;
      pause.setAttribute('aria-pressed', paused ? 'true' : 'false');
      pause.textContent = paused ? 'Resume updates' : 'Pause updates';
    }
  }

  // Whether someone typed or chose something the page does not hold yet.
  function edited(region) {
    return Array.prototype.some.call(region.querySelectorAll('input, textarea, select'), function (field) {
      if (field.tagName === 'SELECT') {
        // Without a `selected` option the page's choice is the first one.
        var chosen = 0;
        for (var i = 0; i < field.options.length; i++) {
          if (field.options[i].defaultSelected) { chosen = i; break; }
        }
        return field.selectedIndex !== chosen;
      }
      return field.type === 'checkbox' || field.type === 'radio' ? field.checked !== field.defaultChecked : field.value !== field.defaultValue;
    });
  }

  // A region is left alone while it is under the pointer, holds the focus,
  // an open disclosure or dialog, or something typed but not sent.
  function settled(region) {
    return !region.matches(':hover') &&
      !(document.activeElement && document.activeElement !== document.body && region.contains(document.activeElement)) &&
      !region.querySelector('details[open], .scrim') &&
      !edited(region);
  }

  function swap(fresh) {
    var waiting = false;
    var regions = document.querySelectorAll('[data-live-region]');
    for (var i = 0; i < regions.length; i++) {
      var region = regions[i];
      var name = region.getAttribute('data-live-region');
      var next = fresh.querySelector('[data-live-region="' + name + '"]');
      if (!next || next.innerHTML === region.innerHTML) continue;
      if (!settled(region)) { waiting = true; continue; }
      var known = {};
      region.querySelectorAll('[data-key]').forEach(function (line) { known[line.getAttribute('data-key')] = true; });
      region.replaceChildren.apply(region, Array.prototype.slice.call(document.importNode(next, true).childNodes));
      if (Object.keys(known).length) {
        region.querySelectorAll('[data-key]').forEach(function (line) {
          if (!known[line.getAttribute('data-key')]) line.classList.add('feed__line--new');
        });
      }
    }
    var freshMeta = fresh.querySelector('.engine__meta');
    if (freshMeta) loaded = freshMeta.textContent;
    setLive(true);
    return waiting;
  }

  function refresh() {
    if (paused || busy) { again = true; return; }
    busy = true;
    again = false;
    fetch(location.href, { credentials: 'same-origin', headers: { Accept: 'text/html' } })
      .then(function (answer) { return answer.ok ? answer.text() : null; })
      .then(function (html) {
        if (html === null) return;
        var waiting = swap(new DOMParser().parseFromString(html, 'text/html'));
        if (say && said) { say.textContent = said; said = ''; }
        if (waiting) again = true;
      })
      .catch(function () { again = true; })
      .then(function () { busy = false; });
  }

  // A reader who leaves a waiting region, or presses Resume, gets the update.
  function left() { setTimeout(function () { if (again && !busy) refresh(); }, 0); }
  document.querySelectorAll('[data-live-region]').forEach(function (region) {
    region.addEventListener('mouseleave', left);
    region.addEventListener('focusout', left);
  });
  document.addEventListener('click', function (event) {
    var pause = event.target.closest && event.target.closest('[data-live-pause]');
    if (!pause) return;
    paused = !paused;
    setLive(true);
    if (!paused && again) refresh();
  });

  function connect() {
    var source = new EventSource('/events');
    source.addEventListener('live', function (event) {
      var news = JSON.parse(event.data);
      wait = 1000;
      meta.textContent = 'Live · ' + news.at;
      setLive(true);
      // Every signed-in page's sidebar shows the Needs review count.
      var mine = news.changed.filter(function (area) { return area === 'count' || shows.indexOf(area) >= 0; });
      if (mine.length) said = 'Updated at ' + news.at;
      // After a drop, read the page once: changes made meanwhile were not sent.
      if (mine.length || dropped) { dropped = false; refresh(); }
    });
    // The stream reached its lifetime: open a new one at once.
    source.addEventListener('bye', function () { source.close(); connect(); });
    source.onerror = function () {
      source.close();
      dropped = true;
      meta.textContent = loaded;
      setLive(false);
      setTimeout(connect, wait);
      wait = Math.min(wait * 2, 30000);
    };
  }
  connect();
}());
