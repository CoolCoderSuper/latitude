export class NeovimGrid {
  constructor() {
    this.width = 0;
    this.height = 0;
    this.cells = [];
    this.highlights = new Map();
    this.foreground = '#d4d4d4';
    this.background = '#181818';
    this.cursor = [0, 0];
    this.modes = [];
    this.mode = {};
    this.busy = false;
  }

  apply(name, args) {
    switch (name) {
      case 'grid_resize': {
        const [grid, width, height] = args;
        if (grid !== 1) break;
        this.cells = Array.from({ length: height }, (_, row) =>
          Array.from(
            { length: width },
            (_, col) => this.cells[row]?.[col] ?? [' ', 0],
          ),
        );
        this.width = width;
        this.height = height;
        break;
      }
      case 'grid_clear':
        if (args[0] === 1)
          this.cells = this.cells.map((row) => row.map(() => [' ', 0]));
        break;
      case 'grid_line': {
        const [grid, row, start, cells] = args;
        if (grid !== 1 || !this.cells[row]) break;
        let col = start;
        let highlight = 0;
        for (const [text, id, repeat = 1] of cells) {
          highlight = id ?? highlight;
          for (let i = 0; i < repeat; i++) {
            if (col < this.width) this.cells[row][col] = [text, highlight];
            col++;
          }
        }
        break;
      }
      case 'grid_scroll': {
        const [grid, top, bottom, left, right, rows, cols] = args;
        if (grid !== 1) break;
        const old = this.cells.map((row) => row.slice());
        for (let row = top; row < bottom; row++) {
          for (let col = left; col < right; col++) {
            const fromRow = row + rows;
            const fromCol = col + cols;
            this.cells[row][col] =
              fromRow >= top &&
              fromRow < bottom &&
              fromCol >= left &&
              fromCol < right
                ? old[fromRow][fromCol]
                : [' ', 0];
          }
        }
        break;
      }
      case 'grid_cursor_goto':
        if (args[0] === 1) this.cursor = args.slice(1);
        break;
      case 'hl_attr_define':
        this.highlights.set(args[0], args[1]);
        break;
      case 'default_colors_set':
        this.foreground = color(args[0], this.foreground);
        this.background = color(args[1], this.background);
        break;
      case 'mode_info_set':
        this.modes = args[1];
        break;
      case 'mode_change':
        this.mode = this.modes[args[1]] ?? {};
        break;
      case 'busy_start':
        this.busy = true;
        break;
      case 'busy_stop':
        this.busy = false;
        break;
    }
  }

  style(id) {
    const attrs = this.highlights.get(id) ?? {};
    let foreground = color(attrs.foreground, this.foreground);
    let background = color(attrs.background, this.background);
    if (attrs.reverse) [foreground, background] = [background, foreground];
    return {
      ...attrs,
      foreground,
      background,
      special: color(attrs.special, foreground),
    };
  }
}

function color(value, fallback) {
  return Number.isInteger(value) && value >= 0
    ? `#${value.toString(16).padStart(6, '0')}`
    : fallback;
}

export function keyInput(event) {
  if (event.isComposing || event.key === 'Process' || event.key === 'Dead')
    return null;
  // Let the browser deliver clipboard and text composition events.
  if (
    (event.metaKey || (event.ctrlKey && event.shiftKey)) &&
    event.key.toLowerCase() === 'v'
  )
    return null;
  const names = {
    Escape: 'Esc',
    Enter: 'CR',
    Backspace: 'BS',
    Delete: 'Del',
    Tab: 'Tab',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
    Insert: 'Insert',
  };
  let key =
    names[event.key] ?? (/^F\d{1,2}$/.test(event.key) ? event.key : null);
  if (!key && !(event.ctrlKey || event.altKey || event.metaKey)) return null;
  if (!key && [...event.key].length !== 1) return null;
  // AltGr produces text through the textarea, rather than Ctrl+Alt bindings.
  if (event.getModifierState?.('AltGraph')) return null;
  key ??= event.key === ' ' ? 'Space' : event.key === '<' ? 'lt' : event.key;
  const modifiers = `${event.ctrlKey ? 'C-' : ''}${event.altKey ? 'A-' : ''}${event.metaKey ? 'D-' : ''}${event.shiftKey ? 'S-' : ''}`;
  return `<${modifiers}${key}>`;
}
