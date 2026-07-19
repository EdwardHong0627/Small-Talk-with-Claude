/*
 * lightbox.js — click a post image to view it enlarged in an overlay.
 * Vanilla JS, no bundler, no framework. Purely client-side: no API calls.
 *
 * Scope: only <img> inside article.prose, and only when the image is not
 * already wrapped in a link (a linked image should follow its href) and not
 * decorative (a missing or empty alt is the standard decorative opt-out).
 *
 * Each enrolled image is wrapped in a real <button class="zoomable-btn">
 * rather than given role="button": the native button supplies the role, the
 * tab order and Enter/Space activation for free, and leaves the <img>'s own
 * implicit role intact so screen-reader image navigation still finds it.
 *
 * While the overlay is open the page behind it (.shell) is made inert, so
 * assistive tech and Tab cannot reach it. aria-modal alone does not do this.
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
  var closeTransitionHandler = null;  // pending fade-out listener, if any

  function prefersReducedMotion() {
    return !!(window.matchMedia &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  }

  function getShell() {
    return document.querySelector('.shell');
  }

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

  // drop any pending fade-out listener so it cannot fire later
  function clearCloseTransitionHandler() {
    if (closeTransitionHandler) {
      overlay.removeEventListener('transitionend', closeTransitionHandler);
      closeTransitionHandler = null;
    }
  }

  // second half of close(): runs once the fade-out has finished (or straight
  // away when transitions are off). Hiding earlier would cancel the fade,
  // because .lightbox[hidden] { display: none } applies synchronously.
  function finishClose() {
    clearCloseTransitionHandler();

    // a re-open beat the fade-out — leave the overlay up
    if (overlay.classList.contains('is-open')) return;

    overlay.hidden = true;

    // release the (possibly large) image so it can be collected
    overlayImg.removeAttribute('src');
    overlayCaption.textContent = '';
  }

  function open(img) {
    if (!overlay) buildOverlay();

    // re-opening mid fade-out must not inherit the pending hide
    clearCloseTransitionHandler();

    lastFocused = document.activeElement;

    // data-full is the author's explicit full-size source. currentSrc is only
    // best-effort: it is whichever srcset variant the browser picked for this
    // viewport, which is not necessarily the largest one.
    overlayImg.src = img.dataset.full || img.currentSrc || img.src;
    overlayImg.alt = img.alt || '';
    overlayCaption.textContent = img.alt || '';

    // measure the scrollbar BEFORE lightbox-open hides it — once the class is
    // on, the viewport is already wider and this reads 0
    var sbw = window.innerWidth - document.documentElement.clientWidth;

    // hide the page behind the dialog from AT and from Tab
    var shell = getShell();
    if (shell) {
      shell.inert = true;
      shell.setAttribute('aria-hidden', 'true');  // fallback for older browsers
    }

    overlay.hidden = false;
    document.body.classList.add('lightbox-open');

    // pad by the width the scrollbar just gave up, so the centred .shell
    // doesn't shift right underneath the overlay
    if (sbw > 0) document.body.style.paddingRight = sbw + 'px';

    // next frame so the opacity transition actually runs
    requestAnimationFrame(function () {
      overlay.classList.add('is-open');
    });

    closeBtn.focus();
  }

  function close() {
    if (!overlay || overlay.hidden) return;

    overlay.classList.remove('is-open');
    document.body.classList.remove('lightbox-open');
    document.body.style.paddingRight = '';

    // restore the page behind us before handing focus back to it
    var shell = getShell();
    if (shell) {
      shell.inert = false;
      shell.removeAttribute('aria-hidden');
    }

    if (lastFocused && typeof lastFocused.focus === 'function') {
      lastFocused.focus();
    }
    lastFocused = null;

    // under prefers-reduced-motion the CSS sets transition: none, so
    // transitionend never fires — hide now or the overlay stays up forever
    if (prefersReducedMotion()) {
      finishClose();
      return;
    }

    clearCloseTransitionHandler();
    closeTransitionHandler = function (e) {
      // transitionend bubbles: .lightbox-close animates its own colours, so
      // ignore anything that isn't the overlay's own opacity fade
      if (e.target !== overlay || e.propertyName !== 'opacity') return;
      finishClose();
    };
    overlay.addEventListener('transitionend', closeTransitionHandler);
  }

  function initLightbox() {
    var images = document.querySelectorAll('article.prose img');
    if (!images.length) return;

    images.forEach(function (img) {
      // a linked image belongs to its link — don't hijack the click
      if (img.closest('a')) return;

      // alt="" (or no alt at all) means decorative — don't enrol it
      if (!img.alt) return;

      // nothing to wrap it in
      if (!img.parentNode) return;

      img.classList.add('zoomable');

      // a real button gives us role, tab order and Enter/Space for free,
      // and leaves the <img>'s implicit role alone
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'zoomable-btn';
      btn.setAttribute('aria-label', 'enlarge image: ' + img.alt);

      img.parentNode.insertBefore(btn, img);
      btn.appendChild(img);

      btn.addEventListener('click', function () {
        open(img);
      });
    });
  }

  // ---------- boot ----------
  document.addEventListener('DOMContentLoaded', initLightbox);
})();
