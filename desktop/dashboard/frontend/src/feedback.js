// A line of feedback beside a control. An error stays until the next action and
// can be copied with a click or the keyboard: one that vanished in two seconds
// couldn't be read, let alone reported (Jack, 12 Nov 2026).

/** Empty the line and take away anything an earlier error left on it. */
export function clearFeedback(el) {
  if (!el) return;
  clearTimeout(el.__t);
  el.__shown = (el.__shown || 0) + 1;
  el.textContent = '';
  el.style.color = 'var(--ink-3)';
  el.style.cursor = '';
  el.title = '';
  el.onclick = null;
  el.onkeydown = null;
  el.removeAttribute('role');
  el.removeAttribute('tabindex');
}

/** Say something that went well, briefly. */
export function showNote(el, msg, ms = 2400) {
  if (!el) return;
  clearFeedback(el);
  el.textContent = msg;
  el.style.color = 'var(--ink-2)';
  el.__t = setTimeout(() => clearFeedback(el), ms);
}

/** Say what went wrong. It stays; a click, Enter or Space copies it. */
export function showError(el, msg) {
  if (!el) return;
  clearFeedback(el);
  const shown = el.__shown;
  el.textContent = msg;
  el.style.color = 'var(--status-bad)';
  el.style.cursor = 'pointer';
  el.title = 'Click to copy';
  el.setAttribute('role', 'button');
  el.setAttribute('tabindex', '0');
  const copy = async () => {
    try { await navigator.clipboard.writeText(msg); } catch { return; /* it stays readable */ }
    // Only if this error is still the one on the line.
    if (el.__shown === shown) el.textContent = `${msg} (copied)`;
  };
  el.onclick = copy;
  el.onkeydown = (e) => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); copy(); }
  };
}
