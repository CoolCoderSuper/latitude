export function normalizeMediaType(mediaType?: string | null): string | null {
  const normalized = mediaType?.split(';')[0]?.trim().toLowerCase();
  return normalized || null;
}

export function isImageMediaType(mediaType?: string | null): boolean {
  return normalizeMediaType(mediaType)?.startsWith('image/') ?? false;
}

export function isVideoMediaType(mediaType?: string | null): boolean {
  return normalizeMediaType(mediaType)?.startsWith('video/') ?? false;
}
