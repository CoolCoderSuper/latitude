// Connections are ordered up, right, down, left.
const connections = [
  [0, 1, 0, 1],
  [1, 0, 1, 0],
  [0, 1, 1, 0],
  [0, 0, 1, 1],
  [1, 1, 0, 0],
  [1, 0, 0, 1],
  [1, 1, 1, 0],
  [1, 0, 1, 1],
  [0, 1, 1, 1],
  [1, 1, 0, 1],
  [1, 1, 1, 1],
];
const glyphs = new Map();
for (const [index, characters] of [
  '─│┌┐└┘├┤┬┴┼',
  '━┃┏┓┗┛┣┫┳┻╋',
  '═║╔╗╚╝╠╣╦╩╬',
].entries()) {
  for (const [position, character] of [...characters].entries())
    glyphs.set(character, { arms: connections[position], weight: index + 1 });
}
for (const [index, character] of [...'╭╮╰╯'].entries())
  glyphs.set(character, {
    arms: connections[index + 2],
    weight: 1,
    rounded: true,
  });

export function drawBoxDrawing(ctx, text, x, y, width, height, scale) {
  const glyph = glyphs.get(text);
  if (!glyph) return false;
  // Font glyphs do not reach the edges of cells with extra line spacing.
  // Use one device-pixel geometry for edges, corners, and junctions.
  const thickness = Math.max(
    1,
    Math.round((glyph.weight === 2 ? 2 : 1) * scale),
  );
  const left = Math.round(x * scale);
  const right = Math.round((x + width) * scale);
  const top = Math.round(y * scale);
  const bottom = Math.round((y + height) * scale);
  const cx =
    Math.round((x + width / 2) * scale - thickness / 2) + thickness / 2;
  const cy =
    Math.round((y + height / 2) * scale - thickness / 2) + thickness / 2;
  const half = thickness / 2;
  const [up, east, down, west] = glyph.arms;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  if (glyph.rounded) {
    const dx = east ? 1 : -1;
    const dy = down ? 1 : -1;
    const radius = Math.max(
      0,
      Math.min(
        3 * scale,
        cx - left - half,
        right - cx - half,
        cy - top - half,
        bottom - cy - half,
      ),
    );
    ctx.strokeStyle = ctx.fillStyle;
    ctx.lineWidth = thickness;
    ctx.lineCap = 'butt';
    ctx.beginPath();
    ctx.moveTo(cx, up ? top : bottom);
    ctx.lineTo(cx, cy + dy * radius);
    ctx.arcTo(cx, cy, cx + dx * radius, cy, radius);
    ctx.lineTo(east ? right : left, cy);
    ctx.stroke();
  } else if (glyph.weight === 3) {
    // Each pair of strokes follows the outside of the connected arms, leaving
    // the space between double lines open through corners and junctions.
    for (const sign of [-1, 1]) {
      const verticalJoin = (sign < 0 ? west : east)
        ? thickness
        : (sign < 0 ? east : west)
          ? -thickness
          : 0;
      const horizontalJoin = (sign < 0 ? up : down)
        ? thickness
        : (sign < 0 ? down : up)
          ? -thickness
          : 0;
      const vx = cx + sign * thickness - half;
      const hy = cy + sign * thickness - half;
      if (up) ctx.fillRect(vx, top, thickness, cy - verticalJoin + half - top);
      if (down) {
        const start = cy + verticalJoin - half;
        ctx.fillRect(vx, start, thickness, bottom - start);
      }
      if (west)
        ctx.fillRect(left, hy, cx - horizontalJoin + half - left, thickness);
      if (east) {
        const start = cx + horizontalJoin - half;
        ctx.fillRect(start, hy, right - start, thickness);
      }
    }
  } else {
    if (up) ctx.fillRect(cx - half, top, thickness, cy + half - top);
    if (east) ctx.fillRect(cx - half, cy - half, right - cx + half, thickness);
    if (down) ctx.fillRect(cx - half, cy - half, thickness, bottom - cy + half);
    if (west) ctx.fillRect(left, cy - half, cx + half - left, thickness);
  }
  ctx.restore();
  return true;
}
