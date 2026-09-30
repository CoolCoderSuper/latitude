import assert from 'node:assert/strict';
import { test } from 'node:test';
import { NeovimGrid, keyInput } from '../src/server/assets/neovim-grid.js';

test('linegrid preserves repeated highlights and wide character continuation cells', () => {
  const grid = new NeovimGrid();
  grid.apply('grid_resize', [1, 6, 2]);
  grid.apply('grid_line', [1, 0, 0, [['界', 7], [''], ['x', 3, 3], ['λ']]]);
  assert.deepEqual(grid.cells[0], [
    ['界', 7],
    ['', 7],
    ['x', 3],
    ['x', 3],
    ['x', 3],
    ['λ', 3],
  ]);
  grid.apply('grid_resize', [1, 7, 3]);
  assert.deepEqual(grid.cells[0][0], ['界', 7]);
  assert.deepEqual(grid.cells[2][6], [' ', 0]);
});

test('scroll copies overlapping rectangles in both directions without touching other windows', () => {
  const grid = new NeovimGrid();
  grid.apply('grid_resize', [1, 4, 4]);
  for (let row = 0; row < 4; row++)
    grid.apply('grid_line', [1, row, 0, [[String(row), 0, 4]]]);
  grid.apply('grid_scroll', [1, 0, 4, 1, 3, 1, 0]);
  assert.deepEqual(
    grid.cells.map((row) => row.map(([text]) => text).join('')),
    ['0110', '1221', '2332', '3  3'],
  );
  grid.apply('grid_scroll', [1, 0, 4, 1, 3, -1, 0]);
  assert.deepEqual(
    grid.cells.map((row) => row.map(([text]) => text).join('')),
    ['0  0', '1111', '2222', '3333'],
  );
});

test('RGB defaults, reverse attributes, and cursor modes follow Neovim events', () => {
  const grid = new NeovimGrid();
  grid.apply('default_colors_set', [0xffffff, 0x123456, -1]);
  grid.apply('hl_attr_define', [
    3,
    { foreground: 0xff0000, reverse: true, bold: true },
  ]);
  assert.equal(grid.style(3).foreground, '#123456');
  assert.equal(grid.style(3).background, '#ff0000');
  grid.apply('mode_info_set', [
    true,
    [{ cursor_shape: 'vertical', cell_percentage: 25 }],
  ]);
  grid.apply('mode_change', ['insert', 0]);
  assert.equal(grid.mode.cursor_shape, 'vertical');
});

test('special keys and modifiers use Neovim notation while text, IME, and paste use browser events', () => {
  assert.equal(keyInput({ key: 'Escape' }), '<Esc>');
  assert.equal(keyInput({ key: 'ArrowLeft', ctrlKey: true }), '<C-Left>');
  assert.equal(keyInput({ key: 'Tab', shiftKey: true }), '<S-Tab>');
  assert.equal(keyInput({ key: 'w', ctrlKey: true }), '<C-w>');
  assert.equal(keyInput({ key: 'λ' }), null);
  assert.equal(keyInput({ key: 'v', ctrlKey: true }), '<C-v>');
  assert.equal(keyInput({ key: 'V', ctrlKey: true, shiftKey: true }), null);
  assert.equal(keyInput({ key: 'v', metaKey: true }), null);
  assert.equal(keyInput({ key: 'Enter', isComposing: true }), null);
  assert.equal(keyInput({ key: 'Alt' }), null);
});
