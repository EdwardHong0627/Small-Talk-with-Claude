/*
 * lightbox.js — click a post image to view it enlarged in an overlay.
 * Vanilla JS, no bundler, no framework. Purely client-side: no API calls.
 *
 * Scope: only <img> inside article.prose, and only when the image is not
 * already wrapped in a link (a linked image should follow its href).
 *
 * XSS RULE: caption text comes from the alt attribute and is set via
 * textContent — never innerHTML.
 */
(function () {
  'use strict';

  var overlay = null;   // built lazily on first open, then reused
  var overlayImg = null;
  var overlayCaption = null;
  var closeBtn = null;
  var lastFocused = null;

  function buildOverlay() {
    overlay = document.createElement('div');
    overlay.className = 'lightbox';
    overlay.hidden = true;
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');
    overlay.setAttribute('aria-label', 'enlarged image');

    closeBtn = document.createElement('button');
    closeBtn.type = 'button';
    closeBtn.className = 'lightbox-close';
    closeBtn.setAttribute('aria-label', 'close enlarged image');
    closeBtn.textContent = '×';

    overlayImg = document.createElement('img');
    overlayImg.alt = '';

    overlayCaption = document.createElement('p');
    overlayCaption.className = 'lightbox-caption';

    overlay.appendChild(closeBtn);
    overlay.appendChild(overlayImg);
    overlay.appendChild(overlayCaption);
    document.body.appendChild(overlay);

    closeBtn.addEventListener('click', close);

    // click anywhere on the backdrop (or the image itself) closes
    overlay.addEventListener('click', function (e) {
      if (e.target === closeBtn) return;
      close();
    });

    // trap focus: the close button is the only focusable thing in here
    overlay.addEventListener('keydown', function (e) {
      if (e.key === 'Tab') {
        e.preventDefault();
        closeBtn.focus();
      }
    });

    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && !overlay.hidden) close();
    });
  }

  function open(img) {
    if (!overlay) buildOverlay();

    lastFocused = document.activeElement;

    // prefer the full-size source if the author supplied a srcset/data-full
    overlayImg.src = img.currentSrc || img.src;
    overlayImg.alt = img.alt || '';
    overlayCaption.textContent = img.alt || '';

    overlay.hidden = false;
    document.body.classList.add('lightbox-open');

    // next frame so the opacity transition actually runs
    requestAnimationFrame(function () {
      overlay.classList.add('is-open');
    });

    closeBtn.focus();
  }

  function close() {
    if (!overlay || overlay.hidden) return;

    overlay.classList.remove('is-open');
    overlay.hidden = true;
    document.body.classList.remove('lightbox-open');

    // release the (possibly large) image so it can be collected
    overlayImg.removeAttribute('src');
    overlayCaption.textContent = '';

    if (lastFocused && typeof lastFocused.focus === 'function') {
      lastFocused.focus();
    }
    lastFocused = null;
  }

  function initLightbox() {
    var images = document.querySelectorAll('article.prose img');
    if (!images.length) return;

    images.forEach(function (img) {
      // a linked image belongs to its link — don't hijack the click
      if (img.closest('a')) return;

      img.classList.add('zoomable');

      // keyboard-reachable: an image is not focusable by default
      img.tabIndex = 0;
      img.setAttribute('role', 'button');
      img.setAttribute('aria-label', img.alt ? ('enlarge image: ' + img.alt) : 'enlarge image');

      img.addEventListener('click', function () {
        open(img);
      });

      img.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          open(img);
        }
      });
    });
  }

  // ---------- boot ----------
  document.addEventListener('DOMContentLoaded', initLightbox);
})();
