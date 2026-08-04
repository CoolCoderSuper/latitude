import {
  LatitudeApiError,
  LatitudePublicApi,
  LatitudeRequestCancelledError,
  absoluteUrl,
  authHeaders,
  normalizeBaseUrl,
} from './api';

describe('LatitudePublicApi', () => {
  afterEach(() => {
    jest.restoreAllMocks();
  });

  it('normalizes base URLs and resolves absolute links', () => {
    expect(normalizeBaseUrl(' latitude.local:8080/// ')).toBe(
      'http://latitude.local:8080',
    );
    expect(absoluteUrl('https://latitude.local/base', '/project')).toBe(
      'https://latitude.local/project',
    );
  });

  it('sends both supported authentication transports', () => {
    expect(authHeaders('token')).toEqual({
      Authorization: 'Bearer token',
      Cookie: 'latitude_public_session=token',
    });
  });

  it('maps API errors to a stable typed error', async () => {
    jest.spyOn(global, 'fetch').mockResolvedValue({
      ok: false,
      status: 403,
      json: async () => ({ error: 'Forbidden' }),
    } as Response);

    await expect(
      new LatitudePublicApi('http://latitude').projects(),
    ).rejects.toEqual(
      expect.objectContaining<Partial<LatitudeApiError>>({
        name: 'LatitudeApiError',
        status: 403,
        message: 'Forbidden',
      }),
    );
  });

  it('distinguishes caller cancellation from network failures', async () => {
    jest.spyOn(global, 'fetch').mockImplementation(
      (_input, init) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new Error('aborted'));
          });
        }),
    );
    const controller = new AbortController();
    const request = new LatitudePublicApi('http://latitude').projects(
      false,
      false,
      controller.signal,
    );
    controller.abort();

    await expect(request).rejects.toBeInstanceOf(LatitudeRequestCancelledError);
  });

  it('updates a deployment archive state through the public API', async () => {
    const fetchMock = jest.spyOn(global, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({}),
    } as Response);

    await new LatitudePublicApi(
      'http://latitude',
      'token',
    ).setDeploymentArchived('demo project', 'preview app', true);

    expect(fetchMock).toHaveBeenCalledWith(
      'http://latitude/__latitude/api/projects/demo%20project/deployments/preview%20app/archive',
      expect.objectContaining({
        body: JSON.stringify({ archived: true }),
        method: 'PATCH',
      }),
    );
  });

  it('searches project files by name or content', async () => {
    const fetchMock = jest.spyOn(global, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ results: [], limited: false }),
    } as Response);

    await new LatitudePublicApi('http://latitude', 'token').searchFiles(
      'demo',
      'needle value',
      'grep',
    );

    expect(fetchMock).toHaveBeenCalledWith(
      'http://latitude/__latitude/api/projects/demo/files?search=needle+value&search_kind=grep',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});
