import { normalizeBaseUrl } from '../api';

export function authenticatedWebSocketUrl({
  baseUrl,
  href,
  parameters,
  token,
}: {
  baseUrl: string;
  href: string;
  parameters?: Record<string, string>;
  token: string;
}): string {
  const cleanHref = href.replace(/\/+$/, '');
  const url = new URL(`${cleanHref}/ws`, `${normalizeBaseUrl(baseUrl)}/`);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  for (const [name, value] of Object.entries(parameters ?? {})) {
    url.searchParams.set(name, value);
  }
  return url.toString();
}

export function withRawMedia(uri: string): string {
  const url = new URL(uri);
  url.searchParams.set('raw', '1');
  return url.toString();
}
