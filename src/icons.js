//! The window's icon set — drawn, not borrowed from the emoji table.
//!
//! One stroke weight (1.7) and round caps across every glyph, so the gear in the
//! titlebar and the arrows in the model stack read as one hand. `currentColor`
//! throughout: an icon takes the colour of whatever it sits in.

const PATHS = {
  // Settings: the machine's control panel — two travel lines with their knobs.
  // A 16px gear turns to mud, and the four-tooth version reads as a sunburst.
  gear: '<path d="M2.5 5.5h11M2.5 10.5h11"/><circle cx="6" cy="5.5" r="1.7"/><circle cx="10.5" cy="10.5" r="1.7"/>',
  close: '<path d="M4 4l8 8M12 4l-8 8"/>',
  up: '<path d="M8 12.5V4M4.2 7.8L8 4l3.8 3.8"/>',
  down: '<path d="M8 3.5V12M11.8 8.2L8 12l-3.8-3.8"/>',
  check: '<path d="M3 8.4l3.2 3.1L13 4.8"/>',
  // Debug log: a prompt — the caret and its line.
  prompt: '<path d="M3.5 5l2.6 3-2.6 3M8.4 11.2h4.1"/>',
  send: '<path d="M8 13V3.5"/><path d="M3.8 7.2 8 3l4.2 4.2"/>',
  // The fallback stack is running on a backup: a bolt, drawn.
  bolt: '<path d="M9 1.8 4 9h3.4l-.6 5.2L12 7H8.5z"/>',
};

// `name` must be a key of PATHS; anything else is a typo, and a silently empty
// icon is harder to spot than a thrown error.
export function icon(name, size = 14) {
  const paths = PATHS[name];
  if (!paths) throw new Error(`no icon "${name}"`);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("width", size);
  svg.setAttribute("height", size);
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.7");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.innerHTML = paths;
  return svg;
}

// Drop an icon into an element that currently holds a text glyph.
export function setIcon(el, name, size = 14) {
  el.textContent = "";
  el.appendChild(icon(name, size));
  return el;
}
