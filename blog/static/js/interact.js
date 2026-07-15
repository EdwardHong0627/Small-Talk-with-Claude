/*
 * interact.js — reader interactivity: reactions, comments, contact form.
 * Vanilla JS, no bundler, no framework. Talks to the blog-api service at
 * /api/* (same-origin in prod; a local dev port in `mdpub preview`).
 *
 * XSS RULE: any user-generated content (comment author/body) MUST be
 * rendered via textContent / createElement — never innerHTML.
 */
(function () {
  'use strict';

  var API = (window.location.port === '1111') ? 'http://127.0.0.1:8787' : '';

  // ---------- client id (used to dedupe/attribute reactions) ----------
  function getClientId() {
    var KEY = 'interact_client_id';
    try {
      var existing = window.localStorage.getItem(KEY);
      if (existing) return existing;
      var id = (window.crypto && typeof window.crypto.randomUUID === 'function')
        ? window.crypto.randomUUID()
        : ('c-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2));
      window.localStorage.setItem(KEY, id);
      return id;
    } catch (e) {
      // localStorage unavailable (e.g. private mode) — fall back to a
      // session-only id so reactions still work for this page view.
      return 'c-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2);
    }
  }

  // ---------- small dom helpers (textContent only, never innerHTML) ----------
  function el(tag, opts) {
    var node = document.createElement(tag);
    opts = opts || {};
    if (opts.className) node.className = opts.className;
    if (opts.text !== undefined && opts.text !== null) node.textContent = opts.text;
    if (opts.attrs) {
      for (var k in opts.attrs) {
        if (Object.prototype.hasOwnProperty.call(opts.attrs, k)) {
          node.setAttribute(k, opts.attrs[k]);
        }
      }
    }
    return node;
  }

  function clearNode(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
  }

  // Turn a non-OK API response into a rejection carrying the server's
  // error message ({"error": "..."}), so forms can show the real reason.
  // The message is only ever assigned via textContent downstream.
  function rejectWithApiError(res) {
    return res.json()
      .catch(function () { return null; })
      .then(function (data) {
        var err = new Error((data && typeof data.error === 'string') ? data.error : '');
        err.isApiError = true;
        throw err;
      });
  }

  function fmtDate(value) {
    if (!value) return '';
    var d = new Date(value);
    if (isNaN(d.getTime())) return String(value);
    return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  }

  // ---------- reactions ----------
  function initReactions(section, slug) {
    var buttons = section.querySelectorAll('.reaction-btn[data-kind]');
    if (!buttons.length) return;

    var counts = {};
    buttons.forEach(function (btn) {
      counts[btn.getAttribute('data-kind')] = 0;
    });

    function renderCounts() {
      buttons.forEach(function (btn) {
        var kind = btn.getAttribute('data-kind');
        var countEl = btn.querySelector('.reaction-count');
        if (countEl) countEl.textContent = String(counts[kind] || 0);
      });
    }

    fetch(API + '/api/reactions?slug=' + encodeURIComponent(slug))
      .then(function (res) { return res.ok ? res.json() : {}; })
      .then(function (data) {
        if (data && typeof data === 'object') {
          Object.keys(data).forEach(function (kind) {
            counts[kind] = data[kind];
          });
        }
        renderCounts();
      })
      .catch(function () { /* leave zeroed counts on failure */ });

    var clientId = getClientId();

    buttons.forEach(function (btn) {
      btn.addEventListener('click', function () {
        var kind = btn.getAttribute('data-kind');
        var prev = counts[kind] || 0;

        // optimistic update
        counts[kind] = prev + 1;
        renderCounts();
        btn.disabled = true;

        fetch(API + '/api/reactions', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ slug: slug, kind: kind, client_id: clientId })
        })
          .then(function (res) {
            if (!res.ok) throw new Error('reaction failed');
          })
          .catch(function () {
            // revert on error
            counts[kind] = prev;
            renderCounts();
          })
          .finally(function () {
            btn.disabled = false;
          });
      });
    });
  }

  // ---------- comments ----------
  function renderComment(list, comment) {
    var item = el('li', { className: 'comment' });

    var head = el('div', { className: 'comment-head' });
    var author = el('span', {
      className: 'comment-author',
      text: (comment && comment.author) ? String(comment.author) : 'anonymous'
    });
    head.appendChild(author);

    if (comment && comment.created_at) {
      var when = el('span', { className: 'comment-date', text: fmtDate(comment.created_at) });
      head.appendChild(when);
    }
    item.appendChild(head);

    var body = el('p', { className: 'comment-body', text: (comment && comment.body) ? String(comment.body) : '' });
    item.appendChild(body);

    list.appendChild(item);
  }

  function loadComments(section, slug) {
    var list = section.querySelector('.comment-list');
    if (!list) return;

    clearNode(list);
    var loading = el('li', { className: 'comment-empty', text: 'loading comments…' });
    list.appendChild(loading);

    fetch(API + '/api/comments?slug=' + encodeURIComponent(slug))
      .then(function (res) { return res.ok ? res.json() : []; })
      .then(function (comments) {
        clearNode(list);
        if (!Array.isArray(comments) || comments.length === 0) {
          list.appendChild(el('li', { className: 'comment-empty', text: 'no comments yet — be the first.' }));
          return;
        }
        comments.forEach(function (c) { renderComment(list, c); });
      })
      .catch(function () {
        clearNode(list);
        list.appendChild(el('li', { className: 'comment-empty', text: 'could not load comments right now.' }));
      });
  }

  function initCommentForm(section, slug) {
    var form = section.querySelector('.comment-form');
    if (!form) return;

    var statusEl = form.querySelector('.comment-status');
    var authorInput = form.querySelector('[name="author"]');
    var bodyInput = form.querySelector('[name="body"]');
    var hpInput = form.querySelector('[name="hp"]');

    form.addEventListener('submit', function (e) {
      e.preventDefault();
      if (statusEl) statusEl.textContent = '';

      var author = authorInput ? authorInput.value.trim() : '';
      var body = bodyInput ? bodyInput.value.trim() : '';
      var hp = hpInput ? hpInput.value : '';

      if (!body) {
        if (statusEl) statusEl.textContent = 'comment body is required.';
        return;
      }

      var submitBtn = form.querySelector('button[type="submit"]');
      if (submitBtn) submitBtn.disabled = true;
      if (statusEl) statusEl.textContent = 'sending…';

      fetch(API + '/api/comments', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ slug: slug, author: author, body: body, hp: hp })
      })
        .then(function (res) {
          if (!res.ok) return rejectWithApiError(res);
          return res.json().catch(function () { return null; });
        })
        .then(function () {
          if (statusEl) statusEl.textContent = 'thanks — your comment is pending moderation.';
          form.reset();
          var inputs = form.querySelectorAll('input, textarea, button');
          inputs.forEach(function (i) { i.disabled = true; });
        })
        .catch(function (err) {
          if (statusEl) {
            statusEl.textContent = (err && err.isApiError && err.message)
              ? err.message
              : 'could not submit comment — please try again later.';
          }
          if (submitBtn) submitBtn.disabled = false;
        });
    });
  }

  // ---------- contact form ----------
  function initContactForm() {
    var form = document.getElementById('contact-form');
    if (!form) return;

    var statusEl = form.querySelector('.contact-status');
    var nameInput = form.querySelector('[name="name"]');
    var emailInput = form.querySelector('[name="email"]');
    var messageInput = form.querySelector('[name="message"]');
    var hpInput = form.querySelector('[name="hp"]');

    form.addEventListener('submit', function (e) {
      e.preventDefault();
      if (statusEl) statusEl.textContent = '';

      var name = nameInput ? nameInput.value.trim() : '';
      var email = emailInput ? emailInput.value.trim() : '';
      var message = messageInput ? messageInput.value.trim() : '';
      var hp = hpInput ? hpInput.value : '';

      if (!email || !message) {
        if (statusEl) statusEl.textContent = 'email and message are required.';
        return;
      }

      var submitBtn = form.querySelector('button[type="submit"]');
      if (submitBtn) submitBtn.disabled = true;
      if (statusEl) statusEl.textContent = 'sending…';

      fetch(API + '/api/contact', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: name, email: email, message: message, hp: hp })
      })
        .then(function (res) {
          if (!res.ok) return rejectWithApiError(res);
          return res.json().catch(function () { return null; });
        })
        .then(function () {
          if (statusEl) statusEl.textContent = 'message sent — thanks for reaching out.';
          form.reset();
          var inputs = form.querySelectorAll('input, textarea, button');
          inputs.forEach(function (i) { i.disabled = true; });
        })
        .catch(function (err) {
          if (statusEl) {
            statusEl.textContent = (err && err.isApiError && err.message)
              ? err.message
              : 'could not send message — please try again later.';
          }
          if (submitBtn) submitBtn.disabled = false;
        });
    });
  }

  // ---------- boot ----------
  document.addEventListener('DOMContentLoaded', function () {
    var section = document.querySelector('section.interact[data-slug]');
    if (section) {
      var slug = section.getAttribute('data-slug');
      if (slug) {
        initReactions(section, slug);
        loadComments(section, slug);
        initCommentForm(section, slug);
      }
    }

    initContactForm();
  });
})();
